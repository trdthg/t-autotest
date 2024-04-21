use crate::error::DriverError;
use crate::server::Server;
use crate::ServerBuilder;
use std::sync::mpsc;
use std::thread;
use std::time;
use t_binding::api::ApiTx;
use t_config::Config;
use t_console::SSH;
use tracing::info;
use tracing::warn;

pub struct Driver {
    pub config: Option<Config>,
    server: Option<Server>,
    pub stop_tx: mpsc::Sender<()>,
    pub msg_tx: ApiTx,
}

type Result<T> = std::result::Result<T, DriverError>;

impl Driver {
    pub fn new(config: Option<Config>) -> Result<Self> {
        let builder = ServerBuilder::new(config.clone());

        let (server, msg_tx, stop_tx) = builder.build().map_err(DriverError::ConsoleError)?;

        let driver = Self {
            config,
            server: Some(server),
            stop_tx,
            msg_tx,
        };
        Ok(driver)
    }

    pub fn start(&mut self) -> &mut Self {
        if let Some(server) = self.server.take() {
            let stop_tx = self.stop_tx.clone();
            if let Err(e) = ctrlc::set_handler(move || {
                let _ = stop_tx.send(());
                thread::sleep(time::Duration::from_secs(2));
                std::process::exit(0);
            }) {
                warn!(msg="set ctrl-c handler failed", reason = ?e);
            }
            server.start_non_blocking();
        }
        self
    }

    pub fn reconnect(&mut self) -> &mut Self {
        // TODO
        self
    }

    pub fn stop(&mut self) -> &mut Self {
        if self.stop_tx.send(()).is_err() {
            tracing::error!("stop server failed");
        }
        self
    }

    pub fn new_ssh(&mut self) -> Result<SSH> {
        if let Some(ssh) = self.config.as_ref().and_then(|c| c.ssh.clone()) {
            SSH::new(ssh).map_err(DriverError::ConsoleError)
        } else {
            Err(DriverError::ConsoleError(
                t_console::ConsoleError::NoConnection("no ssh config".to_string()),
            ))
        }
    }
}
