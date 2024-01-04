use clap::Parser;
use serde::Deserialize;
use std::fs;

#[derive(clap::Parser, Debug)]
pub struct Cli {
    #[clap(short, long)]
    file: String,
}

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
    pub password: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ConsoleSerial {
    pub enable: bool,
    pub serial_file: String,
}

#[derive(Deserialize, Debug)]
pub struct ConsoleVNC {
    pub enable: bool,
    pub ip: String,
    pub port: u16,
    pub password: Option<String>,
}

fn main() -> () {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    // CLI options are defined later in this file
    let cli = Cli::parse();
    println!("{:#?}", cli);
    let config: Config = toml::from_str(fs::read_to_string(&cli.file).unwrap().as_str()).unwrap();
    println!("{:#?}", config);
}
