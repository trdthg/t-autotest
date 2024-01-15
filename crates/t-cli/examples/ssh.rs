use clap::Parser;
use std::path::PathBuf;
use t_console::{SSHAuthAuth, SSHClient};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

fn main() -> () {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // CLI options are defined later in this file
    let cli = Cli::parse();

    info!(
        "Connecting to {}@{}:{}",
        cli.username.clone().unwrap_or(default_username()),
        cli.host,
        cli.port
    );
    info!(
        "Key path: {:?}",
        cli.private_key.clone().unwrap_or(default_private_key())
    );

    // Session is a wrapper around a russh client, defined down below
    let mut ssh = SSHClient::connect(
        None,
        SSHAuthAuth::PrivateKey(cli.private_key.unwrap_or(default_private_key())),
        cli.username.unwrap_or(default_username()),
        (cli.host, cli.port),
    )
    .unwrap();
    info!("Connected");

    let command_str = &cli
        .command
        .iter()
        .map(|x| x.as_ref()) // arguments are escaped manually since the SSH protocol doesn't support quoting
        .collect::<Vec<_>>()
        .join(";");
    let code = ssh.exec_seperate(command_str).unwrap();
    println!("Exitcode: {:?}", code);

    let code = ssh.exec_seperate(command_str).unwrap();
    println!("Exitcode: {:?}", code);
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_username() -> String {
    "root".to_string()
}

fn default_private_key() -> PathBuf {
    match home::home_dir() {
        Some(mut path) => {
            path.push(".ssh");
            path.push("id_rsa");
            path
        }
        None => panic!("no home directory, can set default private key path"),
    }
}

#[derive(clap::Parser)]
#[clap(trailing_var_arg = true)]
pub struct Cli {
    #[clap(long, default_value_t = default_host())]
    host: String,

    #[clap(long, short, default_value_t = 22)]
    port: u16,

    #[clap(long, short)]
    username: Option<String>,

    #[clap(long, short = 'k')]
    private_key: Option<PathBuf>,

    #[clap(index = 1, required = true)]
    command: Vec<String>,
}
