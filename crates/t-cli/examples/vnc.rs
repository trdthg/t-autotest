use clap::Parser;
use log::info;
use t_lib::VNCClient;

fn default_host() -> String {
    "localhost".to_string()
}

#[derive(clap::Parser)]
#[clap(trailing_var_arg = true)]
pub struct Cli {
    #[clap(long, default_value_t = default_host())]
    host: String,

    #[clap(long, short, default_value_t = 5900)]
    port: u16,

    #[clap(long, short)]
    password: Option<String>,

    #[clap(index = 1, required = true)]
    command: Vec<String>,
}

fn main() -> () {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    // CLI options are defined later in this file
    let cli = Cli::parse();

    info!(
        "Connecting to {}:{} password: {:?}",
        cli.host.clone(),
        cli.port,
        cli.password
    );

    let vnc = VNCClient::connect((cli.host, cli.port), cli.password).unwrap();
}
