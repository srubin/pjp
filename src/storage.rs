extern crate directories;
use log::{debug, info};
use serde::{Deserialize, Serialize};

use std::fs::{create_dir_all, File};

use directories::ProjectDirs;

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct PjpConfig {
    pub port: String,
    pub last_fm_api_key: Option<String>,
    pub last_fm_username: Option<String>,
    pub last_fm_password: Option<String>,
    pub last_fm_secret_key: Option<String>,
}

impl Default for PjpConfig {
    fn default() -> Self {
        PjpConfig {
            port: "7878".into(),
            last_fm_api_key: None,
            last_fm_username: None,
            last_fm_password: None,
            last_fm_secret_key: None,
        }
    }
}

pub fn load_config() -> PjpConfig {
    let proj_dirs = ProjectDirs::from("com", "srubin", "pjp").unwrap();
    let config_dir = proj_dirs.config_dir();
    let config_path = config_dir.join("config.json");

    match File::open(config_path.clone()) {
        Ok(config_file) => {
            let config: PjpConfig = serde_json::from_reader(config_file).unwrap();
            info!(
                "loaded config from {}",
                config_path.to_str().unwrap(),
            );
            config
        }
        Err(_) => {
            info!("creating and saving default config");
            let config = PjpConfig::default();
            save_config(&config).unwrap();
            config
        }
    }
}

pub fn save_config(config: &PjpConfig) -> Result<(), Box<dyn std::error::Error>> {
    let proj_dirs = ProjectDirs::from("com", "srubin", "pjp").unwrap();
    let config_dir = proj_dirs.config_dir();
    create_dir_all(config_dir)?;

    let config_path = config_dir.join("config.json");

    println!("config_path: {:?}", config_path);
    let config_file = File::create(config_path)?;
    serde_json::to_writer(config_file, &config)?;
    Ok(())
}

pub fn load_json<T>(name: &str) -> Result<T, Box<dyn std::error::Error>>
where
    for<'de> T: Deserialize<'de>,
{
    let proj_dirs = ProjectDirs::from("com", "srubin", "pjp").unwrap();
    let data_local_dir = proj_dirs.data_local_dir();
    create_dir_all(data_local_dir)?;
    let path: std::path::PathBuf = data_local_dir.join(format!("{}.json", name));
    debug!("loading {}", path.to_str().unwrap());
    let file = File::open(path.clone())?;
    let res = serde_json::from_reader::<File, T>(file)?;
    debug!("loaded {}", path.to_str().unwrap());
    Ok(res)
}

pub fn save_json<T>(name: &str, data: &T) -> Result<(), Box<dyn std::error::Error>>
where
    T: Serialize,
{
    let proj_dirs = ProjectDirs::from("com", "srubin", "pjp").unwrap();
    let data_local_dir = proj_dirs.data_local_dir();
    create_dir_all(data_local_dir)?;
    let path = data_local_dir.join(format!("{}.json", name));
    debug!("saving {}", path.to_str().unwrap());
    let file = File::create(path.clone())?;
    serde_json::to_writer(file, data)?;
    debug!("saved {}", path.to_str().unwrap());
    Ok(())
}
