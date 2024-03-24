use std::{error::Error, fmt::Display};

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
pub enum ApiError {
    ServerStopped,
    ServerInvalidResponse,
    String(String),
    Timeout,
    AssertFailed,
    Interrupt,
}

impl Error for ApiError {}

impl Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::ServerStopped => write!(f, "server stopped, maybe needle not found"),
            ApiError::ServerInvalidResponse => {
                write!(f, "server returned invalid msg type, please report issue")
            }
            ApiError::String(s) => write!(f, "error, {}", s),
            ApiError::Timeout => write!(f, "command timeout"),
            ApiError::AssertFailed => write!(f, "assert command failed, like return code != 0"),
            ApiError::Interrupt => write!(f, "interrupted by signal"),
        }
    }
}
