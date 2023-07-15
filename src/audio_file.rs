use crate::audio_source::{AudioBuffer, AudioMetadata, AudioSource};
use std::borrow::BorrowMut;
use std::ffi::OsString;
use std::fs::File;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSourceStream};
use symphonia::core::meta::{MetadataBuilder, MetadataOptions, StandardTagKey};
use symphonia::core::probe::Hint;
use symphonia_metadata::id3v2::read_id3v2;

pub struct AudioFileSource {
    pub filename: OsString,
    format: Option<Box<dyn FormatReader>>,
    decoder: Option<Box<dyn Decoder>>,
    track_id: Option<u32>,
    decoded_buffers: Vec<AudioBuffer>,
    seek_pos: u32,
    metadata: Option<AudioMetadata>,
}

impl AudioFileSource {
    pub fn new(filename: OsString) -> AudioFileSource {
        AudioFileSource {
            filename,
            format: None,
            decoder: None,
            track_id: None,
            decoded_buffers: Vec::new(),
            seek_pos: 0,
            metadata: None,
        }
    }

    fn make_decoder(&self) -> (Box<dyn FormatReader>, Box<dyn Decoder>, u32) {
        // Create a media source. Note that the MediaSource trait is automatically implemented for File,
        // among other types.
        let file = Box::new(File::open(&self.filename).unwrap());

        // Create the media source stream using the boxed media source from above.
        let mss = MediaSourceStream::new(file, Default::default());

        // Create a hint to help the format registry guess what format reader is appropriate. In this
        // example we'll leave it empty.
        let hint = Hint::new();

        // Use the default options when reading and decoding.
        let format_opts: FormatOptions = Default::default();
        let metadata_opts: MetadataOptions = Default::default();
        let decoder_opts: DecoderOptions = Default::default();

        // Probe the media source stream for a format.
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .unwrap();

        // Get the format reader yielded by the probe operation.
        let format = probed.format;

        // Get the default track.
        let track = format.default_track().unwrap();

        // Create a decoder for the track.
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &decoder_opts)
            .unwrap();

        let track_id = track.id;

        (format, decoder, track_id)
    }
}

impl AudioSource for AudioFileSource {
    fn get_buffer(&mut self, offset: u32) -> Option<&AudioBuffer> {
        // FIXME: factor out this duplicated code
        // find an existing decoded buffer
        // FIXME: O(n), fix
        for i in 0..self.decoded_buffers.len() {
            let buffer = &self.decoded_buffers[i];
            if buffer.offset <= offset && buffer.offset + buffer.length > offset {
                // println!("found existing buffer at offset {}", offset);
                return Some(&self.decoded_buffers[i]);
            }
        }

        if self.format.is_none() || self.decoder.is_none() || self.track_id.is_none() {
            let (format, decoder, track_id) = self.make_decoder();
            self.format = Some(format);
            self.decoder = Some(decoder);
            self.track_id = Some(track_id);
            self.seek_pos = 0;
        }

        let decoder = self.decoder.as_mut().unwrap();
        let format = self.format.as_mut().unwrap();
        let track_id = self.track_id.unwrap();

        // only seek if we're decently far away from the seek pos?
        if offset != self.seek_pos {
            self.seek_pos = match format.seek(
                symphonia::core::formats::SeekMode::Accurate,
                symphonia::core::formats::SeekTo::TimeStamp {
                    ts: offset as u64,
                    track_id,
                },
            ) {
                Ok(seek_to) => seek_to.actual_ts as u32,
                Err(_) => {
                    println!("seek failed");
                    return None;
                }
            };
        }
        // println!("seekedTo: {:?}", seekTo);

        loop {
            // find an existing decoded buffer
            // FIXME: O(n), fix
            for i in 0..self.decoded_buffers.len() {
                let buffer = &self.decoded_buffers[i];
                if buffer.offset <= offset && buffer.offset + buffer.length > offset {
                    return Some(&self.decoded_buffers[i]);
                }
            }

            // Get the next packet from the format reader.
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(_) => {
                    return None;
                }
            };

            // If the packet does not belong to the selected track, skip it.
            if packet.track_id() != track_id {
                continue;
            }

            // Decode the packet into audio samples, ignoring any decode errors.
            match decoder.decode(&packet) {
                Ok(audio_buf) => {
                    // The decoded audio samples may now be accessed via the audio buffer if per-channel
                    // slices of samples in their native decoded format is desired. Use-cases where
                    // the samples need to be accessed in an interleaved order or converted into
                    // another sample format, or a byte buffer is required, are covered by copying the
                    // audio buffer into a sample buffer or raw sample buffer, respectively. In the
                    // example below, we will copy the audio buffer into a sample buffer in an
                    // interleaved order while also converting to a f32 sample format.

                    // FIXME: re-use the sample buf

                    // Get the audio buffer specification.
                    let spec = *audio_buf.spec();

                    // Get the capacity of the decoded buffer. Note: This is capacity, not length!
                    let duration = audio_buf.capacity() as u64;

                    // Create the f32 sample buffer.
                    let mut sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));

                    let channel_count = spec.channels.count();

                    let mut samples = Vec::new();
                    for _channel in 0..channel_count {
                        samples.push(Vec::new());
                    }

                    let mut signal = AudioBuffer {
                        samples,
                        sample_rate: 44100.0,
                        length: 1024,
                        offset,
                    };

                    // Copy the decoded audio buffer into the sample buffer in an interleaved format.
                    if let Some(buf) = &mut sample_buf {
                        buf.copy_planar_ref(audio_buf);

                        let sample_count = buf.samples().len();

                        let samples_per_channel = sample_count / channel_count;

                        for channel in 0..channel_count {
                            let samples = buf.samples();
                            let channel_samples = &samples[channel * samples_per_channel
                                ..(channel + 1) * samples_per_channel];
                            for sample in channel_samples {
                                signal.samples[channel].push(*sample);
                            }
                        }

                        signal.length = samples_per_channel as u32;
                        signal.offset = self.seek_pos;

                        self.seek_pos += samples_per_channel as u32;

                        // println!(
                        //     "\rDecoded {} samples, offset {}",
                        //     sample_count, signal.offset
                        // );

                        self.decoded_buffers.push(signal);

                        // only keep ~5 seconds in memory
                        // 2 * 5 * 44100 / 2000  ~ 220
                        while self.decoded_buffers.len() > 220 {
                            // println!("evicting buffer");
                            self.decoded_buffers.remove(0);
                        }
                    }
                }
                Err(Error::DecodeError(_)) => {}
                Err(_) => panic!("error decoding packet"),
            }
        }
    }

    fn get_metadata(&mut self) -> &AudioMetadata {
        match self.metadata {
            Some(ref metadata) => return metadata,
            None => {
                let mut codec_params: Option<CodecParameters> = None;

                if self.format.is_none() || self.track_id.is_none() {
                    let (format, decoder, track_id) = self.make_decoder();
                    codec_params = Some(format.tracks()[track_id as usize].codec_params.clone());
                    self.format = Some(format);
                    self.decoder = Some(decoder);
                    self.track_id = Some(track_id);
                } else {
                    let format = self.format.as_ref().unwrap();
                    let track_id = self.track_id.unwrap();
                    codec_params = Some(format.tracks()[track_id as usize].codec_params.clone());
                }

                let codec_params = codec_params.unwrap();
                let time_base = codec_params.time_base.unwrap();
                let n_frames = codec_params.n_frames.unwrap();
                let time = time_base.calc_time(n_frames);
                let dur = time.seconds as f64 + time.frac as f64;

                let mut metadata = AudioMetadata {
                    dur,
                    artist: String::from(""),
                    title: self.filename.to_str().unwrap().to_string(),
                    album: String::from(""),
                };

                let mut meta = MetadataBuilder::new();
                let file = Box::new(File::open(&self.filename).unwrap());
                let mut mss = MediaSourceStream::new(file, Default::default());
                if read_id3v2(mss.borrow_mut(), meta.borrow_mut()).is_ok() {
                    let m = meta.metadata();
                    for tag in m.tags() {
                        match tag.std_key {
                            Some(StandardTagKey::TrackTitle) => {
                                metadata.title = tag.value.to_string();
                            }
                            Some(StandardTagKey::Artist) => {
                                metadata.artist = tag.value.to_string();
                            }
                            Some(StandardTagKey::Album) => {
                                metadata.album = tag.value.to_string();
                            }
                            _ => {}
                        }
                    }
                }

                self.metadata = Some(metadata);
                &self.metadata.as_ref().unwrap()
            }
        }
    }
}
