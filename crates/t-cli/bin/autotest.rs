use clap::Parser;
use std::{env, fs, path::Path};
use t_cli::{init, Config, Runner};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(clap::Parser, Debug)]
pub struct Cli {
    #[clap(short, long)]
    file: String,

    #[clap(short, long)]
    case: String,
}

fn main() {
    let format = tracing_subscriber::fmt::format()
        .without_time()
        .with_target(false)
        .with_level(true)
        .with_source_location(true)
        .compact();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(match env::var("RUST_LOG") {
            Ok(l) => match l.as_str() {
                "trace" => Level::TRACE,
                "debug" => Level::DEBUG,
                "warn" => Level::WARN,
                "error" => Level::ERROR,
                "info" | _ => Level::INFO,
            },
            _ => Level::INFO,
        })
        .event_format(format)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();
    info!("{:#?}", cli);

    let config: Config = toml::from_str(fs::read_to_string(&cli.file).unwrap().as_str()).unwrap();
    info!("{:#?}", config);
    let log_dir = config
        .log_dir
        .clone()
        .unwrap_or_else(|| env::current_dir().unwrap().to_str().unwrap().to_string());

    init(config);

    let mut runner = Runner::new(cli.case);
    runner.run();

    runner.dump_log(&Path::new(&log_dir));
}

#[cfg(test)]
mod test {}
