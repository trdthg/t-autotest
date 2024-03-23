mod base;
mod serial;
mod ssh;
mod term;
mod vnc;

use std::fmt::Display;

pub use serial::Serial;
pub use ssh::SSH;
pub use term::*;
pub use vnc::{key, Rect, VNCError, VNCEventReq, VNCEventRes, PNG, VNC};

#[derive(Debug)]
pub enum ConsoleError {
    ConnectionBroken(String),
    Timeout,
}

impl Display for ConsoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ConsoleError::ConnectionBroken(r) => write!(f, "Connection broken, [{}]", r),
            ConsoleError::Timeout => write!(f, "Timeout"),
        }
    }
}

// magic string, used for regex extract in ssh or serial output
#[allow(dead_code)]
static MAGIC_STRING: &str = "n8acxy9o47xx7x7xw";
