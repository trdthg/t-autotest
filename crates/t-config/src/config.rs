use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub log_dir: Option<String>,
    pub console: Console,
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
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConsoleVNC {
    pub enable: bool,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
}

#[cfg(test)]
mod test {}
