mod audio_file;
mod audio_source;
mod player_state;
mod storage;
mod web_framework;

use audio_source::{AudioMetadata, AudioSource};
use coreaudio::audio_unit::render_callback::{self, data};
use coreaudio::audio_unit::{AudioUnit, IOType, SampleFormat};
use log::{error, info};
use player_state::*;
use serde::Serialize;
use serde_json;
use std::borrow::BorrowMut;

use std::net::TcpListener;

use std::sync::{Arc, Mutex};
use std::thread;
use web_framework::{HttpMethod, HttpResponseCode};

use crate::storage::save_json;

#[derive(Serialize)]
struct PlayerStatusResponse<'a> {
    state: String,
    current_item: usize,
    current_offset: f64,
    playlist: Vec<&'a AudioMetadata>,
}

// Abstraction:
// - list of items to play
// - prefetches those items into a buffer
//   - start prefetching multiple items in the list (first few seconds?) so skipping songs is instant
//   - memory goals: only the current audio source is in memory, plus the first few seconds of all other files
// - fetches the next buffer from the current item, and plays that
// - moves onto the next item when the current item is done

fn run_pjp() -> Result<(), coreaudio::Error> {
    let config = storage::load_config();
    let mut player_state = match storage::load_json::<PlayerState>("player_state") {
        Ok(ps) => ps,
        Err(err) => {
            println!("error loading player state: {}", err);
            PlayerState::default()
        }
    };
    player_state.validate();

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

        let _current_item = locked_ps.current_item;

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

                // if the playlist is empty, fill with silence
                if locked_ps.playlist.len() == 0 {
                    for channel in data.channels_mut() {
                        for i in 0..channel.len() {
                            channel[i] = 0.0;
                        }
                    }
                    return Ok(());
                }

                let current_item = locked_ps.current_item;
                let mut current_offset = locked_ps.current_offset;

                let src = locked_ps.playlist[current_item].borrow_mut();

                let mut signal = match src.get_buffer(current_offset) {
                    Some(s) => s,
                    None => {
                        // next track
                        // FIXME: gapless
                        locked_ps.next();
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
                                locked_ps.next();
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

    let address = format!("0.0.0.0:{}", config.port);

    let listener = TcpListener::bind(address.clone()).unwrap();

    info!("listening on {}", address);

    let save_loop_ps = player_state_mutex.clone();
    thread::spawn(move || {
        // save every 30 seconds
        loop {
            thread::sleep(std::time::Duration::from_secs(30));
            let save_res = save_json("player_state", &save_loop_ps);
            if save_res.is_err() {
                error!("error saving player state: {:?}", save_res);
            }
        }
    });

    for stream in listener.incoming() {
        let mut should_save = false;
        let mut stream = stream.unwrap();

        {
            let mut player_state = ps.lock().unwrap();

            let (req, mut res) = web_framework::handle_connection(stream.borrow_mut());

            match req {
                Ok(req) => match (&req.method, req.path.as_str(), &req) {
                    (HttpMethod::Get, "/status", _) => {
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
                        res.response_code = HttpResponseCode::Ok;
                    }
                    (HttpMethod::Post, "/clear", _) => {
                        player_state.clear();
                        should_save = true;
                        res.response_code = HttpResponseCode::Ok;
                    }
                    (HttpMethod::Post, "/next", _) => {
                        player_state.next();
                        should_save = true;
                        res.response_code = HttpResponseCode::Ok;
                    }
                    (HttpMethod::Post, "/pause", _) => {
                        player_state.pause();
                        should_save = true;
                        res.response_code = HttpResponseCode::Ok;
                    }
                    (HttpMethod::Post, "/play", _) => {
                        player_state.play();
                        should_save = true;
                        res.response_code = HttpResponseCode::Ok;
                    }
                    (HttpMethod::Post, "/toggle", _) => {
                        player_state.toggle();
                        should_save = true;
                        res.response_code = HttpResponseCode::Ok;
                    }
                    (HttpMethod::Post, "/add", req) => {
                        match serde_json::from_str(req.body.as_str()) {
                            Ok(paths) => {
                                player_state.add_tracks(paths);
                                should_save = true;
                                res.response_code = HttpResponseCode::Ok;
                            }
                            Err(err) => {
                                error!("error parsing json: {} {}", err, req.body);
                                res.response_code = HttpResponseCode::BadRequest;
                            }
                        }
                    }
                    (HttpMethod::Post, "/skip-to", req) => {
                        match serde_json::from_str(req.body.as_str()) {
                            Ok(index) => {
                                player_state.skip_to(index);
                                should_save = true;
                                res.response_code = HttpResponseCode::Ok;
                            }
                            Err(err) => {
                                error!("error parsing json: {} {}", err, req.body);
                                res.response_code = HttpResponseCode::BadRequest;
                            }
                        }
                    }
                    _ => {
                        res.response_code = HttpResponseCode::NotFound;
                    }
                },
                Err(_) => {
                    error!("error parsing request");
                    res.response_code = HttpResponseCode::InternalServerError;
                }
            }
        } // player_state lock scope ends here

        if should_save {
            let save_res = save_json("player_state", &ps);
            if save_res.is_err() {
                error!("error saving player state: {:?}", save_res);
            }
        }
    }

    Ok(())
}

fn main() {
    env_logger::init();
    run_pjp().unwrap();
}
