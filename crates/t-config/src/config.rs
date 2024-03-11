use serde::Deserialize;
use std::{collections::HashMap, time::Duration};

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub machine: String,
    pub arch: String,
    pub os: String,
    pub needle_dir: String,
    pub console: Console,
    pub env: HashMap<String, toml::Value>,
}

impl Config {
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Console {
    pub ssh: ConsoleSSH,
    pub serial: ConsoleSerial,
    pub vnc: ConsoleVNC,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConsoleSSH {
    pub enable: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: ConsoleSSHAuth,
    pub timeout: Option<Duration>,
    pub log_file: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub enum ConsoleSSHAuthType {
    PrivateKey,
    Password,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConsoleSSHAuth {
    pub r#type: ConsoleSSHAuthType,
    pub private_key: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConsoleSerial {
    pub enable: bool,
    pub serial_file: String,
    pub bund_rate: u32,
    pub auto_login: bool,
    pub username: Option<String>,
    pub password: Option<String>,
    pub log_file: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConsoleVNC {
    pub enable: bool,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub screenshot_dir: Option<String>,
}

#[cfg(test)]
mod test {}
