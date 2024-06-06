use std::sync::{
    mpsc::{self, Sender},
    Arc,
};

use t_binding::api::ApiTx;
use t_config::Config;
use t_console::SSH;
use tracing::warn;

use crate::{
    error::DriverError,
    server::{Server, Service},
};
use t_util::AMOption;

pub struct Driver {
    pub config: Option<Config>,
    pub stop_tx: mpsc::Sender<Sender<()>>,
    pub msg_tx: ApiTx,
    server: Option<Server>,
}

impl Driver {
    pub fn start(&mut self) -> &mut Self {
        if let Some(server) = self.server.take() {
            let stop_tx = self.stop_tx.clone();
            if let Err(e) = ctrlc::set_handler(move || {
                let (tx, rx) = mpsc::channel();
                if stop_tx.send(tx).is_err() || rx.recv().is_err() {
                    tracing::error!("stop server failed");
                    std::process::exit(1);
                }
                std::process::exit(0);
            }) {
                warn!(msg="set ctrl-c handler failed", reason = ?e);
            }
            server.start_non_blocking();
        }
        self
    }

    pub fn stop(&self) {
        let (tx, rx) = mpsc::channel();
        if self.stop_tx.send(tx).is_err() {
            tracing::error!("stop server failed");
        }
        if let Err(e) = rx.recv() {
            tracing::error!(msg="stop server failed", reason = ?e);
        }
    }

    pub fn new_ssh(&mut self) -> StdResult<SSH, DriverError> {
        if let Some(ssh) = self.config.as_ref().and_then(|c| c.ssh.clone()) {
            SSH::new(ssh).map_err(DriverError::ConsoleError)
        } else {
            Err(DriverError::ConsoleError(
                t_console::ConsoleError::NoConnection("no ssh config".to_string()),
            ))
        }
    }
}

pub struct DriverBuilder {
    pub config: Option<Config>,
    disable_screenshot: bool,
}

type StdResult<T, E> = std::result::Result<T, E>;

impl DriverBuilder {
    pub fn new(config: Option<Config>) -> Self {
        Self {
            config,
            disable_screenshot: false,
        }
    }

    pub fn disable_screenshot(mut self) -> Self {
        self.disable_screenshot = true;
        self
    }

    pub fn build(self) -> StdResult<Driver, DriverError> {
        // init api request channel
        let (msg_tx, msg_rx) = mpsc::channel();

        // init stop tx
        let (stop_tx, stop_rx) = mpsc::channel();

        let server = Server {
            msg_rx,
            stop_rx,

            repo: Arc::new(Service {
                enable_screenshot: true,
                config: AMOption::new(self.config.clone()),
                ssh: AMOption::new(None),
                serial: AMOption::new(None),
                vnc: AMOption::new(None),
            }),
        };

        // try connect for the first time
        if let Some(ref c) = self.config {
            server
                .repo
                .connect_with_config(c.clone())
                .map_err(DriverError::ConsoleError)?;
        }

        let driver = Driver {
            config: self.config,
            stop_tx,
            msg_tx,
            server: Some(server),
        };
        Ok(driver)
    }
}
