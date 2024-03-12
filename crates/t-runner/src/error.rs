use std::fmt::Display;

use t_console::ConsoleError;

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
