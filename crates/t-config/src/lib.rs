mod config;

use std::{env, error::Error, fmt::Display, fs, io, path::Path};

pub use config::*;

#[derive(Debug)]
pub enum ConfigError {
    ConfigFileNotFound(io::Error),
    DeserializeFailed(toml::de::Error),
}

impl Error for ConfigError {}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::ConfigFileNotFound(e) => write!(f, "{}", e.to_string()),
            ConfigError::DeserializeFailed(e) => write!(f, "{}", e.to_string()),
        }
    }
}

pub fn load_config_from_file(f: impl AsRef<Path>) -> Result<Config, ConfigError> {
    println!("{:?}", f.as_ref().display());
    println!("{:?}", env::current_dir().unwrap());
    let f = fs::read_to_string(f).map_err(|e| ConfigError::ConfigFileNotFound(e))?;
    toml::from_str::<Config>(f.as_str()).map_err(|e| ConfigError::DeserializeFailed(e))
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
