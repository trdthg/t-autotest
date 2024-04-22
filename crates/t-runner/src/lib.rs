mod driver;
mod driver_for_script;
mod engine;
pub mod needle;
mod server;
pub use driver_for_script::DriverForScript;
pub mod error;
pub use driver::{Driver, DriverBuilder};
use std::fmt::Display;

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
