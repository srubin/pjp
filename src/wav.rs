use crate::{
    audio_source::{AudioBuffer, AudioSource},
    wav_header::WavHeader,
};

use std::collections::HashMap;
use std::{
    ffi::OsString,
    io::{Read, Seek},
};

pub struct WavSource {
    pub filename: OsString,
    header: Option<WavHeader>,
    decoded_buffers: HashMap<u32, AudioBuffer>,
}

impl WavSource {
    pub fn new(filename: OsString) -> WavSource {
        WavSource {
            filename,
            header: None,
            decoded_buffers: HashMap::new(),
        }
    }

    fn read_header(&self) -> Result<WavHeader, Box<dyn std::error::Error>> {
        let mut file = std::fs::File::open(&self.filename)?;
        let mut header = [0u8; 1024];
        file.read_exact(&mut header)?;
        let header = WavHeader::from(header.to_vec());
        Ok(header)
    }
}

impl AudioSource for WavSource {
    fn get_buffer(&mut self, offset: u32) -> Option<&AudioBuffer> {
        let header = match self.header {
            Some(ref header) => header,
            None => {
                let header = self.read_header().unwrap();
                self.header = Some(header);
                self.header.as_ref().unwrap()
            }
        };

        if header.format_type != 1 {
            panic!("only PCM is supported right now");
        }

        let data_start = header.data_start();
        let data_size = header.data_size as usize;

        let sample_count: usize = 1024;

        let byte_start = data_start + offset as usize * header.bytes_per_frame as usize;
        let byte_end = (byte_start + sample_count * header.bytes_per_frame as usize)
            .min(data_start + data_size as usize);

        if byte_start >= byte_end {
            return None;
        }

        // use already-decoded buffer if possible
        let quantized_offset = (offset / sample_count as u32) * sample_count as u32;
        if self.decoded_buffers.contains_key(&quantized_offset) {
            return Some(&self.decoded_buffers[&quantized_offset]);
        }

        // sample_count = sample_count.min((byte_end - byte_start) / header.bytes_per_frame as usize);

        let mut file = std::fs::File::open(&self.filename).unwrap();
        let mut buffer = vec![0u8; byte_end - byte_start as usize];

        println!("reading from file {} {}", byte_start, byte_end);
        file.seek(std::io::SeekFrom::Current(byte_start as i64))
            .unwrap();
        file.read_exact(&mut buffer).unwrap();

        let mut samples = vec![];
        for _channel_i in 0..header.number_of_channels {
            samples.push(vec![0.0; sample_count as usize]);
        }

        let mut signal = crate::audio_source::AudioBuffer {
            samples,
            sample_rate: header.sample_rate as f64,
            length: sample_count as u32,
            offset,
        };

        let bytes_per_sample = header.bits_per_sample as usize / 8;

        for (channel_i, channel_samples) in signal.samples.iter_mut().enumerate() {
            for i in 0..channel_samples.len() {
                let sample_i = (i as usize) * bytes_per_sample * header.number_of_channels as usize
                    + channel_i * bytes_per_sample;

                if sample_i >= buffer.len() - 1 {
                    // the rest is silence
                    break;
                }

                channel_samples[i] = match header.bits_per_sample {
                    16 => {
                        // s16le
                        i16::from_le_bytes([buffer[sample_i], buffer[sample_i + 1]]) as f32
                            / 32768.0
                    }
                    32 => {
                        // f32le
                        f32::from_le_bytes([
                            buffer[sample_i],
                            buffer[sample_i + 1],
                            buffer[sample_i + 2],
                            buffer[sample_i + 3],
                        ])
                    }
                    _ => panic!("unsupported bits per sample {}", header.bits_per_sample),
                };
            }
        }

        self.decoded_buffers.insert(offset, signal);

        Some(&self.decoded_buffers[&offset])
    }
}

#[cfg(test)]
mod tests {
    use crate::{audio_source::AudioSource, wav::WavSource};
    use std::path::PathBuf;

    #[test]
    fn reads_wav_header_from_file() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("resources/ports.wav");

        let wav_src = WavSource::new(d.into_os_string());
        let header = wav_src.read_header().unwrap();

        // let header = super::WavHeader::from(header_vec);
        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.format_type, 1);
        assert_eq!(header.number_of_channels, 1);
        assert_eq!(header.bits_per_sample, 16);
        assert_eq!(header.data_size, 328982);
    }

    #[test]
    fn gets_initial_buffer() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("resources/ports.wav");

        let mut wav_src = WavSource::new(d.into_os_string());
        let buf = wav_src.get_buffer(0).unwrap();

        assert_eq!(buf.samples.len(), 1);
        assert_eq!(buf.length, 1024);
        assert_eq!(buf.offset, 0);
        assert_eq!(buf.sample_rate, 44100.0);
        let mut non_zero_count = 0;
        for sample in buf.samples[0].iter() {
            if *sample != 0.0 {
                non_zero_count += 1;
            }
        }
        assert!(non_zero_count as f32 / buf.length as f32 > 0.95);
    }

    #[test]
    fn gets_silence_after_end_of_file() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("resources/ports.wav");

        let mut wav_src = WavSource::new(d.into_os_string());
        let buf = wav_src.get_buffer(44100 * 10).unwrap();

        assert_eq!(buf.samples.len(), 1);
        assert_eq!(buf.length, 1024);
        assert_eq!(buf.offset, 44100 * 10);
        assert_eq!(buf.sample_rate, 44100.0);
        for sample in buf.samples[0].iter() {
            assert_eq!(*sample, 0.0);
        }
    }
}
