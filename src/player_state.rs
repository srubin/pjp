use serde::{Deserialize, Serialize};

use crate::audio_file::{self, AudioFileSource};

// TODO?: could be AudioSource in theory, but serialization doesn't make as much sense for all formats.
// The use case right now is just playing files, anyway.
type Playlist = Vec<AudioFileSource>;

#[derive(Serialize, Deserialize)]
pub enum PlaybackState {
    Playing,
    Paused,
}

#[derive(Serialize, Deserialize)]
pub struct PlayerState {
    pub state: PlaybackState,
    pub playlist: Playlist,
    pub current_item: usize,
    pub current_offset: u32,
}

impl PlayerState {
    pub fn new() -> Self {
        PlayerState {
            state: PlaybackState::Paused,
            playlist: vec![],
            current_item: 0,
            current_offset: 0,
        }
    }

    pub fn next(&mut self) -> &mut Self {
        self.current_offset = 0;
        self.current_item = (self.current_item + 1) % self.playlist.len();
        self
    }

    pub fn pause(&mut self) -> &mut Self {
        self.state = PlaybackState::Paused;
        self
    }

    pub fn play(&mut self) -> &mut Self {
        self.state = PlaybackState::Playing;
        self
    }

    pub fn toggle(&mut self) -> &mut Self {
        match self.state {
            PlaybackState::Paused => self.play(),
            PlaybackState::Playing => self.pause(),
        }
    }

    pub fn add_tracks(&mut self, paths: Vec<String>) -> &mut Self {
        for path in paths {
            let src = audio_file::AudioFileSource::new(path.into());
            self.playlist.push(src);
        }
        self
    }
}
