use std::borrow::BorrowMut;

use serde::{Deserialize, Serialize};

use crate::{
    audio_file::{self, AudioFileSource},
    audio_source::{AudioMetadata, AudioSource},
};

// TODO?: could be AudioSource in theory, but serialization doesn't make as much sense for all formats.
// The use case right now is just playing files, anyway.
type Playlist = Vec<AudioFileSource>;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerState {
    pub state: PlaybackState,
    pub playlist: Playlist,
    pub current_item: usize,
    pub current_offset: u32,
    pub consume: bool,
}

#[derive(Serialize)]
pub struct NowPlaying<'a> {
    pub track: &'a AudioMetadata,
    pub elapsed: f64,
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            state: PlaybackState::Paused,
            playlist: vec![],
            current_item: 0,
            current_offset: 0,
            consume: true,
        }
    }
}

impl PlayerState {
    pub fn new() -> Self {
        PlayerState::default()
    }

    pub fn clear(&mut self) -> &mut Self {
        self.playlist.clear();
        self.current_item = 0;
        self.current_offset = 0;
        self
    }

    pub fn next(&mut self) -> &mut Self {
        if self.playlist.len() > 0 {
            self.current_offset = 0;
            if self.consume {
                self.playlist.remove(self.current_item);
            } else {
                self.current_item = (self.current_item + 1) % self.playlist.len();
            }
        }
        self
    }

    pub fn skip_to(&mut self, index: usize) -> &mut Self {
        if index < self.playlist.len() && index < self.current_item {
            // skipping to a previous song; never consume
            self.current_item = index;
            self.current_offset = 0;
        } else if index > self.current_item {
            let diff = index - self.current_item;
            for _ in 0..diff {
                // toggle consume behavior if necessary
                self.next();
            }
        } else {
            // same track, reset playhead
            self.current_offset = 0;
        }
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
        self.validate();
        self
    }

    /// Remove all non-existent tracks from the playlist
    pub fn validate(&mut self) -> &mut Self {
        self.playlist
            .retain(|src| std::path::Path::new(&src.filename).exists());
        self
    }

    pub fn now_playing(&mut self) -> Option<NowPlaying> {
        if self.playlist.len() > 0 && self.state == PlaybackState::Playing {
            let playlist: &mut Playlist = self.playlist.borrow_mut();
            let track = playlist.get_mut(self.current_item).unwrap();
            Some(NowPlaying {
                track: track.get_metadata(),
                elapsed: self.current_offset as f64 / 44100.0,
            })
        } else {
            None
        }
    }
}
