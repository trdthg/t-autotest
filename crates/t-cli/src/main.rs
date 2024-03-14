pub mod recorder;

use clap::{Parser, Subcommand};
use std::{env, fs, io::IsTerminal, path::Path, sync::mpsc};
use t_config::Config;
use t_runner::{DriverForScript, ServerBuilder};
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(clap::Parser, Debug)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Run {
        #[clap(short, long)]
        config: String,

        #[clap(short, long)]
        script: String,

        #[clap(long)]
        env: Vec<String>,
    },
    Record {
        #[clap(short, long)]
        config: String,
    },
}

fn main() {
    let format = tracing_subscriber::fmt::format()
        .without_time()
        .with_target(false)
        .with_level(true)
        .with_ansi(std::io::stdout().is_terminal())
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

    match cli.command {
        Commands::Run {
            config,
            script,
            env,
        } => {
            let mut config: Config =
                toml::from_str(fs::read_to_string(config).unwrap().as_str()).unwrap();
            info!(msg = "current config", config = ?config);

            for e in env {
                if let Some((key, value)) = e.split_once('=') {
                    config
                        .env
                        .insert(key.to_string(), toml::Value::String(value.to_string()));
                }
            }

            let ext = Path::new(script.as_str())
                .extension()
                .unwrap()
                .to_string_lossy()
                .to_string();

            match DriverForScript::new_with_engine(config, ext) {
                Ok(mut d) => {
                    d.start().run_file(script).stop();
                }
                Err(e) => {
                    error!(msg = "Driver init failed", reason = ?e)
                }
            }
        }
        Commands::Record { config } => {
            let config: Config =
                toml::from_str(fs::read_to_string(config).unwrap().as_str()).unwrap();
            info!(msg = "current config", config = ?config);

            let _vnc = &config.console.vnc;
            if !_vnc.enable {
                warn!("Please enable vnc in your config.toml");
                return;
            }

            let (screenshot_tx, screenshot_rx) = mpsc::channel();
            let builder = ServerBuilder::new(config).with_vnc_screenshot_subscriber(screenshot_tx);

            match builder.build() {
                Ok((server, stop_tx)) => {
                    server.start();
                    recorder::RecorderBuilder::new(stop_tx, screenshot_rx)
                        .build()
                        .start();
                }
                Err(e) => {
                    error!(msg = "Driver init failed", reason = ?e)
                }
            }
        }
    }
}
