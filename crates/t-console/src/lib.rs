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

pub type Result<T> = std::result::Result<T, ConsoleError>;

#[derive(Debug)]
pub enum ConsoleError {
    NoConnection(String),
    NoBashSupport(String),
    //
    Timeout,
    Cancel,
    // other error
    IO(std::io::Error),
    Serial(serialport::Error),
    SSH2(ssh2::Error),
}

impl Display for ConsoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ConsoleError::NoConnection(s) => write!(f, "connection failed: {}", s),
            ConsoleError::Timeout => write!(f, "Timeout"),
            ConsoleError::Cancel => write!(f, "Cancel"),
            ConsoleError::NoBashSupport(s) => write!(f, "no bash support, {}", s),
            ConsoleError::IO(e) => write!(f, "io error, {}", e),
            ConsoleError::SSH2(e) => write!(f, "ssh error, {}", e),
            ConsoleError::Serial(e) => write!(f, "serial error, {}", e),
        }
    }
}

// magic string, used for regex extract in ssh or serial output
#[allow(dead_code)]
static MAGIC_STRING: &str = "n8acxy9o47xx7x7xw";
