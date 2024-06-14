use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf, time::Duration};

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub machine: Option<String>,
    pub arch: Option<String>,
    pub os: Option<String>,

    pub log_dir: Option<String>,
    pub env: Option<HashMap<String, toml::Value>>,

    pub ssh: Option<ConsoleSSH>,
    pub serial: Option<ConsoleSerial>,
    pub vnc: Option<ConsoleVNC>,
}

impl Config {
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        let mut config: Config = toml::from_str(s)?;
        config.init();
        Ok(config)
    }

    fn init(&mut self) {
        let log_dir = self.log_dir.clone().unwrap_or("log".to_string());
        if let Some(serial) = self.serial.as_mut() {
            serial.log_file = Some(PathBuf::from_iter(vec![&log_dir, "serial.log"]));
        }
        if let Some(ssh) = self.ssh.as_mut() {
            ssh.log_file = Some(PathBuf::from_iter(vec![&log_dir, "ssh.log"]));
        }
        if let Some(vnc) = self.vnc.as_mut() {
            vnc.screenshot_dir = Some(PathBuf::from_iter(vec![&log_dir, "vnc"]));
            fs::create_dir_all(vnc.screenshot_dir.clone().unwrap())
                .expect("log folder create failed");
        }
        fs::create_dir_all(log_dir.as_str()).expect("log folder create failed");
        self.log_dir = Some(log_dir);
    }

    pub fn from_toml_file(s: &str) -> Result<Self, toml::de::Error> {
        let mut config: Config = toml::from_str(fs::read_to_string(s).unwrap().as_str()).unwrap();
        config.init();
        Ok(config)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConsoleSSH {
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub timeout: Option<Duration>,
    pub enable_echo: Option<bool>,
    pub linebreak: Option<String>,

    #[serde(skip_serializing)]
    pub log_file: Option<PathBuf>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConsoleSerial {
    pub serial_file: String,
    pub bund_rate: Option<u32>,
    pub r#type: Option<ConsoleSerialType>,
    pub disable_echo: Option<bool>,
    pub linebreak: Option<String>,

    #[serde(skip_serializing)]
    pub log_file: Option<PathBuf>,
}

#[derive(Deserialize, Debug, Clone)]
pub enum ConsoleSerialType {
    Pts,
    Sock,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConsoleVNC {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub needle_dir: Option<String>,

    #[serde(skip_serializing)]
    pub screenshot_dir: Option<PathBuf>,
}

#[cfg(test)]
mod test {}
