pub struct SineSource {
    pub freqs: Vec<f32>,
    buffer: Option<AudioBuffer>,
    metadata: AudioMetadata,
}

impl AudioSource for SineSource {
    fn get_buffer(&mut self, offset: u32) -> Option<&AudioBuffer> {
        let mut signal = AudioBuffer {
            samples: vec![vec![0.0; 1024], vec![0.0; 1024]],
            sample_rate: 44100.0,
            length: 1024,
            offset,
        };
        sine_wave(&self.freqs, &mut signal);
        self.buffer = Some(signal);
        Some(&self.buffer.as_ref().unwrap())
    }

    fn get_metadata(&mut self) -> &audio_source::AudioMetadata {
        &self.metadata
    }
}

fn sine_wave(freqs: &Vec<f32>, signal: &mut AudioBuffer) {
    // FIXME: rewrite this as an iterator?
    let amplitude = 0.1;
    for (channel_i, channel_samples) in signal.samples.iter_mut().enumerate() {
        let freq = freqs[channel_i % freqs.len()];
        for i in 0..channel_samples.len() {
            let t = (i as f32 + signal.offset as f32) / signal.sample_rate as f32;
            let sample = amplitude * (2.0 * PI * freq * t).sin();
            channel_samples[i as usize] = sample;
        }
    }
}
