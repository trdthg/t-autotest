use clap::Parser;
use std::fs;
use t_cli::{init, Config};
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
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();
    info!("{:#?}", cli);

    let config: Config = toml::from_str(fs::read_to_string(&cli.file).unwrap().as_str()).unwrap();
    info!("{:#?}", config);

    init(config);

    info!("init done");
}

#[cfg(test)]
mod test {}
