mod storage;

use std::{borrow::BorrowMut, collections::HashMap};

use log::{debug, info};
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

    #[serde(skip)]
    client: Option<reqwest::blocking::Client>,
}

impl Scrobbler {
    fn post<T: for<'a> Deserialize<'a>>(
        &mut self,
        method: &str,
        params: HashMap<&str, &str>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let mut params = params.clone();
        params.insert("method", method);
        params.insert("api_key", self.api_key.as_str());

        let signature = make_signature(&params, self.secret_key.as_str());

        params.insert("api_sig", signature.as_str());
        params.insert("format", "json");

        let client = match self.client {
            Some(ref client) => client,
            None => {
                let client = reqwest::blocking::Client::new();
                self.client = Some(client);
                self.client.as_ref().unwrap()
            }
        };

        let res = client
            .post("https://ws.audioscrobbler.com/2.0/")
            .form(&params)
            .send()?;

        let body = res.text()?;

        debug!("body: {}", body);

        Ok(serde_json::from_str(&body)?)
    }

    fn get<T: for<'a> Deserialize<'a>>(
        &mut self,
        method: &str,
        params: HashMap<&str, &str>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let mut params = params.clone();
        params.insert("method", method);
        params.insert("api_key", self.api_key.as_str());
        params.insert("format", "json");

        let client = match self.client {
            Some(ref client) => client,
            None => {
                let client = reqwest::blocking::Client::new();
                self.client = Some(client);
                self.client.as_ref().unwrap()
            }
        };

        let res = client
            .get("https://ws.audioscrobbler.com/2.0/")
            .query(&params)
            .send()?;

        let body = res.text()?;

        debug!("body: {}", body);

        Ok(serde_json::from_str(&body)?)
    }

    /// Example last.fm get request
    /// https://www.last.fm/api/show/user.getLovedTracks
    pub fn get_loved_tracks(&mut self) -> Result<GetLovedTracksResult, Box<dyn std::error::Error>> {
        let username = self.username.clone();
        Ok(self.borrow_mut().get::<GetLovedTracksResult>(
            "user.getLovedTracks",
            HashMap::from([("user", username.as_str())]),
        )?)
    }
}

/// Following the auth procedure here: https://www.last.fm/api/mobileauth
fn fetch_token(
    username: &str,
    password: &str,
    api_key: &str,
    secret_key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut params = HashMap::new();
    params.insert("method".into(), "auth.getMobileSession".into());
    params.insert("username".into(), username);
    params.insert("password".into(), password);
    params.insert("api_key".into(), api_key);

    let signature = make_signature(&params, secret_key);

    let client = reqwest::blocking::Client::new();
    let res = client
        .post("https://ws.audioscrobbler.com/2.0/")
        .form(&[
            ("method", "auth.getMobileSession"),
            ("password", &password),
            ("username", &username),
            ("api_key", &api_key),
            ("api_sig", &signature),
            ("format", "json"),
        ])
        .send()?;

    let body = res.text()?;

    let res: AuthGetMobileSessionResult = serde_json::from_str(&body)?;
    Ok(res.session.key)
}

/// Following the signature procedure here: https://www.last.fm/api/mobileauth
fn make_signature(parameters: &HashMap<&str, &str>, secret: &str) -> String {
    // sort the parameter keys alphabetically
    let mut keys: Vec<&str> = parameters.keys().map(|k| *k).collect();
    keys.sort();

    // concatenate the parameters into a string
    let mut signature = String::new();
    for key in keys {
        signature.push_str(&key);
        signature.push_str(parameters.get(&key).unwrap());
    }

    signature.push_str(&secret);

    let digest = md5::compute(signature);
    format!("{digest:x}")
}

impl Scrobbler {
    pub fn try_new() -> Result<Self, Box<dyn std::error::Error>> {
        let scrobbler = storage::load_json::<Scrobbler>("last_fm_session");
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
                )?;
                let scrobbler = Scrobbler {
                    token,
                    username,
                    api_key,
                    secret_key,
                    client: None,
                };
                storage::save_json("last_fm_session", &scrobbler)?;
                info!("fetched new last.fm session");
                Ok(scrobbler)
            }
            _ => {
                return Err(
                    "last.fm api key, secret, username, and password must be set in config".into(),
                )
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

fn main() {}
