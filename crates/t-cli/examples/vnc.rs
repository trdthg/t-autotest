use clap::Parser;
use t_console::VNC;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

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

    #[clap(long, short = 'P')]
    password: Option<String>,

    #[clap(long)]
    command: Vec<String>,
}

fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // CLI options are defined later in this file
    let cli = Cli::parse();

    info!(
        "Connecting to {}:{} password: {:?}",
        cli.host.clone(),
        cli.port,
        cli.password
    );

    VNC::connect(
        format!("{}:{}", cli.host, cli.port).parse().unwrap(),
        cli.password,
        None,
    )
    .unwrap();
}
