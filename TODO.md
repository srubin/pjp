# pjp

pjp ([Pepper's Jam][pj] Player) is a replacement for ([my particular use cases of][use-cases]) mpd.

[pj]: https://peppersjam.com
[use-cases]: http://ssrubin.com/posts/music-library-with-mpd-ncmpcpp-beets.html

### Todo

Roughly prioritized (highest at top)

- [ ] Save playlist when closing
- [ ] Refactor to separate web server from the player. Getting playlist metadata shouldn't interfere with playback, but right now it does because that operation is on the shared player state mutex
  - [ ] Bug: occasional glitching during playback. Potential culprits: decoding in the render loop (do more prefetching),
    - From copilot: audio unit buffer size (try increasing), audio unit render thread priority (try increasing), audio unit render thread scheduling (try real-time scheduling)
- [ ] Bug: connecting bluetooth headphones while playing causes audio to stop (sometimes)
- [ ] Close audio unit when not playing
- [ ] Gapless playback between tracks
- [ ] Prefetch first 5 seconds of every song in the playlist for instant track skipping
- [ ] Tiny crossfade when switching tracks?
- [ ] Tune track buffer cache
- [ ] Scheduling system for determining when to do work (e.g., reading file tags) without affecting the playback thread? Only matters right now because we're locking the entire player state during the audio unit render callback. We probably don't need to do that.
- [ ] Tests...?
- [ ] Robust handling of output and file sample rates

### In progress

- [ ] Scrobble plays to last.fm to replace mpdscribble
  - [x] Decide on architecture. Separate executable, getting data from pjp via the pjp web server? SSE? (Going with separate executable and SSE)
  - [ ] Write up arch
  - [x] Implement MVP
  - [ ] Simplify control flow. Job queue w/ backoff for sending data to last.fm, etc.)? State machine?
  - [ ] unit tests
- [ ] [Raycast](https://www.raycast.com/) extension for controlling pjp
  - [x] next / toggle / add song / add album
  - [x] playlist listing
  - [x] skip-to in playlist
  - [x] Replace mpc search with beets search
  - [ ] Set loved track in last.fm
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
