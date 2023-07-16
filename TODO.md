# pjp

pjp ([Pepper's Jam][pj] Player) is a replacement for ([my particular use cases of][use-cases]) mpd.

[pj]: https://peppersjam.com
[use-cases]: http://ssrubin.com/posts/music-library-with-mpd-ncmpcpp-beets.html

### Todo

Roughly prioritized (highest at top)

- [ ] Scrobble plays to last.fm to replace mpdscribble
- [ ] Close audio unit when not playing
- [ ] Gapless playback between tracks
- [ ] Prefetch first 5 seconds of every song in the playlist for instant track skipping
- [ ] Refactor to separate web server from the player
- [ ] Tiny crossfade when switching tracks?
- [ ] Tune track buffer cache
- [ ] Scheduling system for determining when to do work (e.g., reading file tags) without affecting the playback thread? Only matters right now because we're locking the entire player state during the audio unit render callback. We probably don't need to do that.
- [ ] Tests...?
- [ ] Robust handling of output and file sample rates

### In progress

- [ ] [Raycast](https://www.raycast.com/) extension for controlling pjp
  - [x] next / toggle / add song / add album
  - [x] playlist listing
  - [ ] Replace mpc search with beets search
  - [ ] include in this repo?
- [ ] Learn rust idioms and best practices

### Done

- [x] Play audio buffers with coreaudio-rs (so, pjp is macOS-only for now)
- [x] next / play / pause
- [x] Sine wave generation and playback (simple AudioSource test)
- [x] WAV file decoding and playback (before I decided to use symphonia for all audio decoding)
- [x] Decode audio files with symphonia
- [x] Smol http server framework to control the player
- [x] Build on github actions
- [x] Config for port
- [x] Playback consume mode (i.e., track is removed from the playlist when it ends)
- [x] Persistent playlist storage
  - [x] Save when adding tracks
  - [x] Save on next
  - [x] Save periodically (30s?)
