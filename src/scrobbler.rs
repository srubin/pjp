use std::{borrow::BorrowMut, collections::HashMap};

use log::info;
use serde::{Deserialize, Serialize};

use crate::storage;

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

#[derive(Serialize, Deserialize)]
pub struct Scrobbler {
    token: String,
    username: String,
    api_key: String,
}

/// Following the auth procedure here: https://www.last.fm/api/mobileauth
fn fetch_token(
    username: String,
    password: String,
    api_key: String,
    secret_key: String,
) -> Result<String, Box<dyn std::error::Error>> {
    // FIXME: error handling

    let mut params = HashMap::new();
    params.insert("method".into(), "auth.getMobileSession".into());
    params.insert("username".into(), username.clone());
    params.insert("password".into(), password.clone());
    params.insert("api_key".into(), api_key.clone());

    let signature = make_signature(params, secret_key);

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
fn make_signature(parameters: HashMap<String, String>, secret: String) -> String {
    // sort the parameter keys alphabetically
    let mut keys: Vec<String> = parameters.keys().map(|k| k.to_string()).collect();
    keys.sort();

    // concatenate the parameters into a string
    let mut signature = String::new();
    for key in keys {
        signature.push_str(&key);
        signature.push_str(&parameters[&key]);
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
                let token = fetch_token(username.clone(), password, api_key.clone(), secret_key)?;
                let scrobbler = Scrobbler {
                    token,
                    username,
                    api_key,
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
        let res = make_signature(map, "xyz".into());
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
