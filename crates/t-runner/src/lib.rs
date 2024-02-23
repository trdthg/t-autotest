mod driver;
mod engine;
mod needle;
mod serial;
mod server;
mod ssh;

use std::fmt::Display;

pub use driver::Driver;
pub use ssh::SSHClient;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[derive(Debug)]
pub enum InteractError {
    ConnectionBroken,
    Timeout,
}

impl std::error::Error for InteractError {}
impl Display for InteractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InteractError::ConnectionBroken => write!(f, "connection broken"),
            InteractError::Timeout => write!(f, "timeout"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
