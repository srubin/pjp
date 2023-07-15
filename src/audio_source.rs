use serde::Serialize;

pub struct AudioBuffer {
    pub samples: Vec<Vec<f32>>,
    pub sample_rate: f64,
    pub length: u32,
    pub offset: u32,
}

#[derive(Serialize)]
pub struct AudioMetadata {
    pub dur: f64,
    pub artist: String,
    pub title: String,
    pub album: String,
}

pub trait AudioSource {
    /// Returns a buffer of audio data to play that contains `offset` sample
    /// FIXME: should this pass sample rate, or should that be handled elsewhere?
    /// FIXME: be explicit about the lifetime of this buffer. when can we re-use it?
    /// Returns None if there is no more audio to play.
    fn get_buffer(&mut self, offset: u32) -> Option<&AudioBuffer>;

    fn get_metadata(&mut self) -> &AudioMetadata;
}
