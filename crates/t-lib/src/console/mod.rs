mod serial;
mod ssh;
mod vnc;

pub use serial::SerialClient;
pub use ssh::SSHClient;
pub use vnc::VNCClient;

pub trait FullPowerConsole: ScreenControlConsole + DuplexChannelConsole {}

pub trait ScreenControlConsole {}

pub trait DuplexChannelConsole {
    fn exec(&mut self, cmd: &str) -> String;
}

#[cfg(test)]
mod test {}
