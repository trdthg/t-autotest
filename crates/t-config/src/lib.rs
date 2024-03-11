mod config;
pub use config::*;
use std::{error::Error, fmt::Display, fs, io, path::Path};

#[derive(Debug)]
pub enum ConfigError {
    ConfigFileNotFound(io::Error),
    DeserializeFailed(toml::de::Error),
}

impl Error for ConfigError {}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::ConfigFileNotFound(e) => write!(f, "{}", e),
            ConfigError::DeserializeFailed(e) => write!(f, "{}", e),
        }
    }
}

pub fn load_config_from_file(f: impl AsRef<Path>) -> Result<Config, ConfigError> {
    let f = fs::read_to_string(f).map_err(ConfigError::ConfigFileNotFound)?;
    toml::from_str::<Config>(f.as_str()).map_err(ConfigError::DeserializeFailed)
}

#[cfg(test)]
mod test {
    use crate::load_config_from_file;

    #[test]
    fn test_example_toml() {
        let cfg = load_config_from_file("../../config/full-example.toml").unwrap();
        println!("{:#?}", cfg);
    }
}
