mod base;
mod serial;
mod ssh;
mod term;
mod vnc;

use std::fmt::Display;

pub use serial::SerialTty;
pub use ssh::SSHPts;
pub use term::*;
pub use vnc::{Rect, VNCClient, VNCError, VNCEventReq, VNCEventRes, PNG};

#[derive(Debug)]
pub enum ConsoleError {
    ConnectionBroken,
    Timeout,
}

impl Display for ConsoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ConsoleError::ConnectionBroken => write!(f, "Connection broken"),
            ConsoleError::Timeout => write!(f, "Timeout"),
        }
    }
}

// magic string, used for regex extract in ssh or serial output
#[allow(dead_code)]
static MAGIC_STRING: &str = "n8acxy9o47xx7x7xw";
