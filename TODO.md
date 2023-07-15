# pjp

pjp ([Pepper's Jam][pj] Player) is a replacement for ([my particular use cases of][use-cases]) mpd.

[pj]: https://peppersjam.com
[use-cases]: http://ssrubin.com/posts/music-library-with-mpd-ncmpcpp-beets.html

### Todo

- [ ] Build on github actions
- [ ] Refactor to separate web server from the player
- [ ] Persistent playlist storage
- [ ] Playback consume mode (i.e., track is removed from the playlist when it ends)
- [ ] Gapless playback between tracks
- [ ] Scrobble plays to last.fm to replace mpdscribble
- [ ] Close audio unit when not playing
- [ ] Tiny crossfade when switching tracks?
- [ ] Prefetch first 5 seconds of every song in the playlist for instant track skipping
- [ ] Tune track buffer cache
- [ ] Scheduling system for determining when to do work (e.g., reading file tags) without affecting the playback thread? Only matters right now because we're locking the entire player state during the audio unit render callback. We probably don't need to do that.
- [ ] Tests...?

### In progress

- [ ] [Raycast](https://www.raycast.com/) extension for controlling pjp
  - [x] next / toggle / add song / add album
  - [ ] Replace mpc search with beets search
- [ ] Learn rust idioms and best practices

### Done

- [x] Play audio buffers with coreaudio-rs (so, pjp is macOS-only for now)
- [x] next / play / pause
- [x] Sine wave generation and playback (simple AudioSource test)
- [x] WAV file decoding and playback (before I decided to use symphonia for all audio decoding)
- [x] Decode audio files with symphonia
- [x] Smol http server framework to control the player
