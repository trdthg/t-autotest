use clap::Parser;
use pipe_trait::Pipe;
use std::{env, fs, path::Path};
use t_config::Config;
use t_runner::Runner;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(clap::Parser, Debug)]
pub struct Cli {
    #[clap(short, long)]
    config: String,

    #[clap(short, long)]
    script: String,

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
        toml::from_str(fs::read_to_string(&cli.config).unwrap().as_str()).unwrap();
    info!(msg = "current config", config = ?config);

    for e in cli.env {
        if let Some((key, value)) = e.split_once('=') {
            config
                .env
                .insert(key.to_string(), toml::Value::String(value.to_string()));
        }
    }

    let script = fs::read_to_string(cli.script.as_str()).expect("script not exists");
    let ext = Path::new(cli.script.as_str())
        .extension()
        .unwrap()
        .to_string_lossy()
        .to_string();

    Runner::new_with_engine(config, ext)
        .start()
        .run_script(script)
        .stop()
        .pipe(|s| {
            info!(msg = "dumping logs");
            s
        })
        .dump_log();
}

#[cfg(test)]
mod test {}
