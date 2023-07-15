mod audio_file;
mod audio_source;
mod web_framework;

use audio_source::{AudioBuffer, AudioMetadata, AudioSource};
use coreaudio::audio_unit::render_callback::{self, data};
use coreaudio::audio_unit::{AudioUnit, IOType, SampleFormat};
use log::info;
use serde::Serialize;
use serde_json;
use std::borrow::BorrowMut;
use std::f32::consts::PI;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use web_framework::{HttpMethod, HttpRequest, HttpResponse, HttpResponseCode};

type Playlist = Vec<Box<dyn AudioSource>>;

enum PlaybackState {
    Playing,
    Paused,
}

struct PlayerState {
    state: PlaybackState,
    playlist: Playlist,
    current_item: usize,
    current_offset: u32,
}

#[derive(Serialize)]
struct PlayerStatusResponse<'a> {
    state: String,
    current_item: usize,
    current_offset: f64,
    playlist: Vec<&'a AudioMetadata>,
}

enum PlayerCommand {
    Next,
    Pause,
    Play,
    Toggle,
    AddTracks { paths: Vec<String> },
    Status,
}

// Abstraction:
// - list of items to play
// - prefetches those items into a buffer
//   - start prefetching multiple items in the list (first few seconds?) so skipping songs is instant
//   - memory goals: only the current audio source is in memory, plus the first few seconds of all other files
// - fetches the next buffer from the current item, and plays that
// - moves onto the next item when the current item is done

fn run_pjp() -> Result<(), coreaudio::Error> {
    // let mut signal_index = 0;

    let mut player_state = PlayerState {
        state: PlaybackState::Playing,
        playlist: vec![],
        current_item: 0,
        current_offset: 0,
    };

    // from: https://github.com/RustAudio/coreaudio-rs/blob/master/examples/sine.rs

    // Construct an Output audio unit that delivers audio to the default output device.
    let mut audio_unit = AudioUnit::new(IOType::DefaultOutput)?;

    // Read the input format. This is counterintuitive, but it's the format used when sending
    // audio data to the AudioUnit representing the output device. This is separate from the
    // format the AudioUnit later uses to send the data to the hardware device.
    let stream_format = audio_unit.input_stream_format()?;

    info!("stream format: {:#?}", &stream_format);

    let channels = stream_format.channels;

    let buffer_size = 1024;

    let mut samples = Vec::new();
    for _ in 0..channels {
        samples.push(vec![0.0; buffer_size]);
    }

    // For this example, our sine wave expects `f32` data.
    assert!(SampleFormat::F32 == stream_format.sample_format);

    let player_state_mutex = Arc::new(Mutex::new(player_state));

    let ps = player_state_mutex.clone();

    type Args = render_callback::Args<data::NonInterleaved<f32>>;
    audio_unit.set_render_callback(move |args| {
        let mut locked_ps = ps.lock().unwrap();

        let current_item = locked_ps.current_item;

        match locked_ps.state {
            PlaybackState::Paused => {
                // fill with silence
                let Args { mut data, .. } = args;
                for channel in data.channels_mut() {
                    for i in 0..channel.len() {
                        channel[i] = 0.0;
                    }
                }
                Ok(())
            }
            PlaybackState::Playing => {
                let Args {
                    num_frames,
                    mut data,
                    ..
                } = args;

                let current_item = locked_ps.current_item;
                let mut current_offset = locked_ps.current_offset;

                let src = locked_ps.playlist[current_item].as_mut();

                let mut signal = match src.get_buffer(current_offset) {
                    Some(s) => s,
                    None => {
                        // next track
                        // FIXME: gapless
                        locked_ps.current_item = (current_item + 1) % locked_ps.playlist.len();
                        locked_ps.current_offset = 0;
                        return Ok(());
                    }
                };

                let mut consumed_frames: u32 = 0;

                while (consumed_frames as usize) < num_frames {
                    if signal.offset + signal.length <= current_offset {
                        // grab the next buffer
                        signal = match src.get_buffer(current_offset) {
                            Some(s) => s,
                            None => {
                                // next track
                                // FIXME: gapless
                                locked_ps.current_item =
                                    (current_item + 1) % locked_ps.playlist.len();
                                locked_ps.current_offset = 0;
                                return Ok(());
                            }
                        };
                    }
                    if signal.offset > current_offset {
                        // panic!
                        // or play nothing
                        consumed_frames += 1;
                        continue;
                    }
                    let signal_index = current_offset - signal.offset;

                    let mut channel_index = 0;
                    for channel in data.channels_mut() {
                        let sample = signal.samples[channel_index % signal.samples.len()]
                            [signal_index as usize];
                        channel[consumed_frames as usize] = sample;
                        channel_index += 1;
                    }
                    consumed_frames += 1;
                    current_offset += 1;
                }

                locked_ps.current_offset = current_offset;

                Ok(())
            }
        }
    })?;
    audio_unit.start()?;

    let ps = player_state_mutex.clone();

    let listener = TcpListener::bind("0.0.0.0:7878").unwrap();
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let mut player_state = ps.lock().unwrap();

        let (req, mut res) = handle_connection(stream.borrow_mut());

        if let Ok(req) = req {
            let (response_code, cmd) = match (&req.method, req.path.as_str(), &req) {
                (HttpMethod::Get, "/status", _) => {
                    (HttpResponseCode::Ok, Some(PlayerCommand::Status))
                }
                (HttpMethod::Post, "/next", _) => (HttpResponseCode::Ok, Some(PlayerCommand::Next)),
                (HttpMethod::Post, "/pause", _) => {
                    (HttpResponseCode::Ok, Some(PlayerCommand::Pause))
                }
                (HttpMethod::Post, "/play", _) => (HttpResponseCode::Ok, Some(PlayerCommand::Play)),
                (HttpMethod::Post, "/toggle", _) => {
                    (HttpResponseCode::Ok, Some(PlayerCommand::Toggle))
                }
                (HttpMethod::Post, "/add", req) => match serde_json::from_str(req.body.as_str()) {
                    Ok(paths) => (
                        HttpResponseCode::Ok,
                        Some(PlayerCommand::AddTracks { paths }),
                    ),
                    Err(err) => {
                        println!("error parsing json: {} {}", err, req.body);
                        (HttpResponseCode::BadRequest, None)
                    }
                },
                _ => (HttpResponseCode::NotFound, None),
            };

            res.response_code = response_code;

            match cmd {
                Some(PlayerCommand::Next) => {
                    player_state.current_offset = 0;
                    player_state.current_item =
                        (player_state.current_item + 1) % player_state.playlist.len();
                }
                Some(PlayerCommand::Pause) => {
                    player_state.state = PlaybackState::Paused;
                }
                Some(PlayerCommand::Play) => {
                    player_state.state = PlaybackState::Playing;
                }
                Some(PlayerCommand::Toggle) => match player_state.state {
                    PlaybackState::Paused => {
                        player_state.state = PlaybackState::Playing;
                    }
                    PlaybackState::Playing => {
                        player_state.state = PlaybackState::Paused;
                    }
                },
                Some(PlayerCommand::AddTracks { paths }) => {
                    for path in paths {
                        let src = audio_file::AudioFileSource::new(path.into());
                        player_state.playlist.push(Box::new(src));
                    }
                }
                Some(PlayerCommand::Status) => {
                    let status = PlayerStatusResponse {
                        state: match player_state.state {
                            PlaybackState::Paused => "paused".to_string(),
                            PlaybackState::Playing => "playing".to_string(),
                        },
                        current_item: player_state.current_item,
                        current_offset: player_state.current_offset as f64 / 44100.0,
                        playlist: player_state
                            .playlist
                            .iter_mut()
                            .map(|src| src.get_metadata())
                            .collect(),
                    };

                    res.set_json(&status);
                }
                None => {}
            }
        } else {
            res.response_code = HttpResponseCode::InternalServerError;
        }
    }

    Ok(())
}

fn handle_connection(stream: &mut TcpStream) -> (Result<HttpRequest, ()>, HttpResponse) {
    let req = HttpRequest::try_from(stream.borrow_mut());
    let res: HttpResponse = HttpResponse::new(stream);
    (req, res)
}

fn main() {
    env_logger::init();
    run_pjp().unwrap();
}
