use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub console: Console,
}

#[derive(Deserialize, Debug)]
pub struct Console {
    pub ssh: ConsoleSSH,
    pub serial: ConsoleSerial,
    pub vnc: ConsoleVNC,
}

#[derive(Deserialize, Debug)]
pub struct ConsoleSSH {
    pub enable: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: ConsoleSSHAuth,
}

#[derive(Deserialize, Debug)]
pub enum ConsoleSSHAuthType {
    PrivateKey,
    Password,
}

#[derive(Deserialize, Debug)]
pub struct ConsoleSSHAuth {
    pub r#type: ConsoleSSHAuthType,
    pub private_key: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ConsoleSerial {
    pub enable: bool,
    pub serial_file: String,
    pub bund_rate: u32,
}

#[derive(Deserialize, Debug)]
pub struct ConsoleVNC {
    pub enable: bool,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
}

#[cfg(test)]
mod test {
    use std::fs;

    use super::Config;

    #[test]
    fn test_example_toml() {
        toml::from_str::<Config>(fs::read_to_string("./config.full.toml").unwrap().as_str())
            .unwrap();
    }
}
