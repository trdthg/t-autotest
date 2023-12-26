mod ssh;
mod vnc;

pub use ssh::SSHClient;

pub trait FullPowerConsole: ScreenControlConsole + DuplexChannelConsole {}

pub trait ScreenControlConsole {}

pub trait DuplexChannelConsole {}

#[cfg(test)]
mod test {}
