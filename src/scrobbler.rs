// TODO: move NowPlaying out of player_state
mod audio_file;
mod audio_source;
mod player_state;
mod storage;

use std::{borrow::BorrowMut, collections::HashMap};

use futures::stream::StreamExt;
use log::{debug, error, info};
use player_state::NowPlaying;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct LastFMToken {
    name: String,
    key: String,
    subscriber: i64,
}

#[derive(Serialize, Deserialize)]
struct AuthGetMobileSessionResult {
    session: LastFMToken,
}

#[derive(Debug, Deserialize)]
struct LastFMArtist {
    name: String,
    mbid: Option<String>,
    url: String,
}

#[derive(Debug, Deserialize)]
struct LastFMDate {
    #[serde(rename = "#text")]
    text: String,
    uts: String,
}

#[derive(Debug, Deserialize)]
struct LastFMError {
    code: String,
    #[serde(rename = "#text")]
    text: String,
}

/// Used for cases where we don't care about the response
#[derive(Debug, Deserialize)]
struct LastFMGenericStatus {
    error: Option<LastFMError>,
}

#[derive(Debug, Deserialize)]
struct LastFMTrack {
    mbid: Option<String>,
    name: String,
    url: String,
    artist: LastFMArtist,
    date: LastFMDate,
}

#[derive(Debug, Deserialize)]
struct LastFMTracks {
    track: Vec<LastFMTrack>,
}

#[derive(Debug, Deserialize)]
pub struct GetLovedTracksResult {
    lovedtracks: LastFMTracks,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Scrobbler {
    token: String,
    username: String,
    api_key: String,
    secret_key: String,

    to_scrobble: Vec<NowPlaying>,

    now_playing_start: Option<NowPlaying>,
    now_playing_end: Option<NowPlaying>,

    #[serde(skip)]
    client: Option<reqwest::Client>,
}

impl Scrobbler {
    async fn post<T: for<'a> Deserialize<'a>>(
        &mut self,
        method: String,
        params: HashMap<String, String>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let mut params = params.clone();
        params.insert("method".to_string(), method);
        params.insert("api_key".to_string(), self.api_key.clone());
        params.insert("sk".to_string(), self.token.clone());

        let signature = make_signature(&params, self.secret_key.as_str());

        params.insert("api_sig".to_string(), signature);
        params.insert("format".to_string(), "json".to_string());

        let client = match self.client {
            Some(ref client) => client,
            None => {
                let client = reqwest::Client::new();
                self.client = Some(client);
                self.client.as_ref().unwrap()
            }
        };

        let res = client
            .post("https://ws.audioscrobbler.com/2.0/")
            .form(&params)
            .send()
            .await?;

        let body = res.text().await?;

        debug!("body: {}", body);

        Ok(serde_json::from_str(&body)?)
    }

    async fn get<T: for<'a> Deserialize<'a>>(
        &mut self,
        method: &str,
        params: HashMap<String, &str>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let mut params = params.clone();
        params.insert("method".to_string(), method);
        params.insert("api_key".to_string(), self.api_key.as_str());
        params.insert("format".to_string(), "json");

        let client = match self.client {
            Some(ref client) => client,
            None => {
                let client = reqwest::Client::new();
                self.client = Some(client);
                self.client.as_ref().unwrap()
            }
        };

        let res = client
            .get("https://ws.audioscrobbler.com/2.0/")
            .query(&params)
            .send();

        let body = res.await?.text().await?;

        debug!("body: {}", body);

        Ok(serde_json::from_str(&body)?)
    }

    /// Example last.fm get request
    /// https://www.last.fm/api/show/user.getLovedTracks
    pub async fn get_loved_tracks(
        &mut self,
    ) -> Result<GetLovedTracksResult, Box<dyn std::error::Error>> {
        let username = self.username.clone();
        self.borrow_mut()
            .get::<GetLovedTracksResult>(
                "user.getLovedTracks",
                HashMap::from([("user".to_string(), username.as_str())]),
            )
            .await
    }

    pub async fn scrobble(&mut self) -> Result<LastFMGenericStatus, Box<dyn std::error::Error>> {
        let rest = if self.to_scrobble.len() > 50 {
            self.to_scrobble.split_off(50)
        } else {
            vec![]
        };

        let mut params = HashMap::new();
        for (i, track) in self.to_scrobble.iter().enumerate() {
            params.insert(format!("artist[{}]", i), track.track.artist.clone());
            params.insert(format!("track[{}]", i), track.track.title.clone());
            params.insert(format!("duration[{}]", i), format!("{}", track.track.dur));
            params.insert(format!("timestamp[{}]", i), format!("{}", track.start_ts));
        }

        let result = self
            .borrow_mut()
            .post::<LastFMGenericStatus>("track.scrobble".to_string(), params)
            .await?;

        match result.error {
            Some(err) => {
                error!("error scrobbling: {:?}", err);

                // https://www.last.fm/api/scrobbling
                if err.code != "11" && err.code != "16" {
                    // failure; don't retry
                    self.to_scrobble = rest;
                }

                Err(err.text.into())
            }
            None => {
                self.to_scrobble = rest;
                Ok(result)
            }
        }
    }

    pub async fn set_now_playing(
        &mut self,
        track: Option<NowPlaying>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut should_try_scrobble = false;
        let mut should_update_now_playing = false;
        let mut should_update_now_playing_end = false;

        if let Some(was_playing) = &self.now_playing_start {
            if let Some(now_playing) = &track {
                if now_playing.track.title == was_playing.track.title
                    && now_playing.track.artist == was_playing.track.artist
                    && now_playing.start_ts == was_playing.start_ts
                    && now_playing.elapsed >= was_playing.elapsed
                {
                    debug!("we're already playing this track");
                    // we're already playing this track
                    should_update_now_playing_end = true;
                } else {
                    should_try_scrobble = true;
                    should_update_now_playing = true;
                }
            } else {
                // we were playing something else, but now we're not playing anything
                should_try_scrobble = true;
                should_update_now_playing = true;
            }
        } else {
            // we weren't playing anything
            should_update_now_playing = true;
        }

        if should_try_scrobble {
            // we were playing something else
            match (&self.now_playing_start, &self.now_playing_end) {
                (Some(was_playing_start), Some(was_playing_end)) => {
                    let total_elapsed = was_playing_end.elapsed - was_playing_start.elapsed;
                    if total_elapsed > 4.0 * 60.0
                        || total_elapsed > 0.5 * was_playing_start.track.dur
                    {
                        // we've played half the track, or more than 4 minutes of it track
                        self.to_scrobble.push(was_playing_start.clone());
                    } else {
                        debug!("not scrobbling, only played {} seconds", total_elapsed);
                    }
                }
                (_, _) => {}
            }
        }

        if should_update_now_playing {
            if let Some(track) = &track {
                match self.send_now_playing(track).await {
                    Ok(_) => debug!("set now playing"),
                    Err(err) => error!("error setting now playing: {}", err),
                }
            }
            self.now_playing_start = track;
            self.now_playing_end = None;
        } else if should_update_now_playing_end {
            self.now_playing_end = track;
        }

        if !self.to_scrobble.is_empty() {
            match self.scrobble().await {
                Ok(_) => debug!("scrobbled"),
                Err(err) => error!("error scrobbling: {}", err),
            }
        }
        Ok(())
    }

    pub async fn send_now_playing(
        &mut self,
        track: &NowPlaying,
    ) -> Result<LastFMGenericStatus, Box<dyn std::error::Error>> {
        let mut params = HashMap::new();
        params.insert("track".to_string(), track.track.title.clone());
        params.insert("artist".to_string(), track.track.artist.clone());
        params.insert("album".to_string(), track.track.album.clone());
        params.insert("duration".to_string(), format!("{}", track.track.dur));

        let result = self
            .borrow_mut()
            .post::<LastFMGenericStatus>("track.updateNowPlaying".to_string(), params)
            .await?;

        match result.error {
            Some(err) => {
                error!("error updating now playing: {:?}", err);
                Err(err.text.into())
            }
            None => Ok(result),
        }
    }
}

/// Following the auth procedure here: https://www.last.fm/api/mobileauth
async fn fetch_token(
    username: &str,
    password: &str,
    api_key: &str,
    secret_key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut params = HashMap::new();
    params.insert("method".to_string(), "auth.getMobileSession".to_string());
    params.insert("username".to_string(), username.to_string());
    params.insert("password".to_string(), password.to_string());
    params.insert("api_key".to_string(), api_key.to_string());

    let signature = make_signature(&params, secret_key);

    let client = reqwest::Client::new();
    let res = client
        .post("https://ws.audioscrobbler.com/2.0/")
        .form(&[
            ("method", "auth.getMobileSession"),
            ("password", password),
            ("username", username),
            ("api_key", api_key),
            ("api_sig", &signature),
            ("format", "json"),
        ])
        .send()
        .await?;

    let body = res.text().await?;

    let res: AuthGetMobileSessionResult = serde_json::from_str(&body)?;
    Ok(res.session.key)
}

/// Following the signature procedure here: https://www.last.fm/api/mobileauth
fn make_signature(parameters: &HashMap<String, String>, secret: &str) -> String {
    // sort the parameter keys alphabetically
    let mut keys = parameters
        .keys()
        .map(|k| k.to_string())
        .collect::<Vec<String>>();
    keys.sort();

    // concatenate the parameters into a string
    let mut signature = String::new();
    for key in keys {
        signature.push_str(&key);
        signature.push_str(parameters.get(&key).unwrap());
    }

    signature.push_str(secret);

    let digest = md5::compute(signature);
    format!("{digest:x}")
}

impl Scrobbler {
    pub async fn try_new() -> Result<Self, Box<dyn std::error::Error>> {
        let scrobbler = storage::load_json::<Scrobbler>("scrobbler");
        let config = storage::load_config();

        if let (Ok(scrobbler), Some(username)) = (scrobbler, config.last_fm_username.clone()) {
            if username == scrobbler.username {
                // we already have a token that matches the username
                info!("using existing last.fm session for user {}", username);
                return Ok(scrobbler);
            }
        }

        match (
            config.last_fm_username,
            config.last_fm_password,
            config.last_fm_api_key,
            config.last_fm_secret_key,
        ) {
            (Some(username), Some(password), Some(api_key), Some(secret_key)) => {
                let token = fetch_token(
                    username.as_str(),
                    password.as_str(),
                    api_key.as_str(),
                    secret_key.as_str(),
                )
                .await?;
                let scrobbler = Scrobbler {
                    token,
                    username,
                    api_key,
                    secret_key,
                    client: None,
                    to_scrobble: vec![],
                    now_playing_start: None,
                    now_playing_end: None,
                };
                storage::save_json("scrobbler", &scrobbler)?;
                info!("fetched new last.fm session");
                Ok(scrobbler)
            }
            _ => {
                Err("last.fm api key, secret, username, and password must be set in config".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::make_signature;

    #[test]
    fn makes_signature() {
        let mut map = HashMap::new();
        map.insert("foo".into(), "bar".into());
        map.insert("baz".into(), "qux".into());
        let res = make_signature(&map, "xyz");
        assert_eq!(res, "5b44ff6a214ae37880ba22083aea0881");
        assert_eq!(res.len(), 32);
    }

    // #[test]
    // fn fetches_token() {
    //     fetch_token(
    //         "".into(),
    //         "".into(),
    //         "".into(),
    //         "".into(),
    //     )
    // }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let config = storage::load_config();

    let mut scrobbler = Scrobbler::try_new().await.unwrap();

    let _ = scrobbler.scrobble().await;

    loop {
        let url = format!("http://127.0.0.1:{}/events", config.port);
        debug!("connecting to {}", url);
        let mut es = EventSource::get(url);
        debug!("created event source");
        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => debug!("connection open"),
                Ok(Event::Message(message)) => match message.event.as_str() {
                    "now-playing" => {
                        let now_playing: NowPlaying = serde_json::from_str(&message.data).unwrap();
                        match scrobbler.set_now_playing(Some(now_playing)).await {
                            Ok(()) => debug!("done processing now playing"),
                            Err(err) => error!("error setting now playing: {}", err),
                        }
                    }
                    "playlist-empty" => {
                        debug!("playlist empty");
                        match scrobbler.set_now_playing(None).await {
                            Ok(()) => debug!("done processing now playing"),
                            Err(err) => error!("error setting now playing: {}", err),
                        }
                    }
                    "paused" => {
                        debug!("paused");
                    }
                    _ => error!("unknown event: {}", message.event),
                },
                Err(err) => {
                    println!("Error: {}", err);
                    es.close();
                }
            }

            let _ = storage::save_json("scrobbler", &scrobbler);
        }

        // TODO: exponential backoff(?)

        // reconnect after 5 seconds
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}
