use clap::Parser;
use std::path::PathBuf;
use t_config::ConsoleSSH;
use t_console::SSH;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

fn main() {
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
    match SSH::new(ConsoleSSH {
        host: cli.host,
        port: Some(cli.port),
        username: cli.username.unwrap_or_else(|| "root".to_string()),
        private_key: cli
            .private_key
            .map(|p| p.as_path().to_string_lossy().to_string()),
        password: cli.password,
        timeout: None,
        log_file: None,
        enable_echo: Some(false),
        linebreak: Some("\n".to_string()),
    }) {
        Ok(mut ssh) => {
            info!("Connected");

            let command_str = &cli
                .command
                .iter()
                .map(|x| x.as_ref()) // arguments are escaped manually since the SSH protocol doesn't support quoting
                .collect::<Vec<_>>()
                .join(";");
            let code = ssh.exec_seperate(command_str).unwrap();
            println!("Exitcode: {:?}", code);
        }
        Err(e) => {
            println!("connect failed: {:?}", e.to_string());
        }
    }
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
        None => panic!("Unable to get your home dir by env HOME"),
    }
}

#[derive(clap::Parser)]
#[clap(trailing_var_arg = true)]
pub struct Cli {
    #[clap(long, default_value_t = default_host())]
    host: String,

    #[clap(long, short, default_value_t = 22)]
    port: u16,

    #[clap(long, short = 'u')]
    username: Option<String>,

    #[clap(long, short = 'k')]
    private_key: Option<PathBuf>,

    #[clap(long, short = 'p')]
    password: Option<String>,

    #[clap(index = 1, required = true)]
    command: Vec<String>,
}
