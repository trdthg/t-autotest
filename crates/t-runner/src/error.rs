use std::{fmt::Display, sync::mpsc};

use t_config::Config;
use t_console::ConsoleError;

use crate::Server;

pub struct Driver {
    pub config: Config,
    server: Server,
    server_stop_tx: mpsc::Sender<()>,
}

#[derive(Debug)]
pub enum DriverError {
    ConsoleError(ConsoleError),
}

// impl Error for DriverError {};
impl Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::ConsoleError(e) => write!(f, "console error, {}", e),
        }
    }
}
