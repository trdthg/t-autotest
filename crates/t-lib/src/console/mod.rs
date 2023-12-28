mod serial;
mod ssh;
mod vnc;

use std::io::{Read, Write};

pub use serial::SerialClient;
pub use ssh::SSHClient;
pub use vnc::VNCClient;

pub trait FullPowerConsole: ScreenControlConsole + DuplexChannelConsole {}

pub trait ScreenControlConsole {}

pub trait DuplexChannelConsole {}

#[cfg(test)]
mod test {}
