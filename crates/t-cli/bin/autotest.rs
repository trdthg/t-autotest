use clap::Parser;
use std::{env, fs};
use t_cli::Runner;
use t_config::Config;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(clap::Parser, Debug)]
pub struct Cli {
    #[clap(short, long)]
    file: String,

    #[clap(short, long)]
    case: String,

    #[clap(long)]
    env: Vec<String>,
}

fn main() {
    let format = tracing_subscriber::fmt::format()
        .without_time()
        .with_target(false)
        .with_level(true)
        .with_ansi(atty::is(atty::Stream::Stdout))
        .with_source_location(true)
        .compact();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(match env::var("RUST_LOG") {
            Ok(l) => match l.as_str() {
                "trace" => Level::TRACE,
                "debug" => Level::DEBUG,
                "warn" => Level::WARN,
                "error" => Level::ERROR,
                _ => Level::INFO,
            },
            _ => Level::INFO,
        })
        .event_format(format)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();
    info!(msg = "current cli", cli = ?cli);

    let mut config: Config =
        toml::from_str(fs::read_to_string(&cli.file).unwrap().as_str()).unwrap();
    info!(msg = "current config", config = ?config);

    for e in cli.env {
        if let Some((key, value)) = e.split_once('=') {
            config
                .env
                .insert(key.to_string(), toml::Value::String(value.to_string()));
        }
    }

    let mut runner = Runner::new(cli.case, config);
    runner.run();

    info!(msg = "uploading logs.....");
    runner.dump_log();
    info!(msg = "done!");
}

#[cfg(test)]
mod test {}
