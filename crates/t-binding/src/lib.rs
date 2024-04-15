pub mod api;
mod engine;
pub mod error;
pub mod msg;

pub use engine::JSEngine;
pub use error::{ApiError, Result};
pub use msg::{MsgReq, MsgRes, MsgResError, TextConsole};

pub enum EngineError {}

pub trait ScriptEngine {
    fn run_file(&mut self, path: &str);
    fn run_string(&mut self, content: &str);
}
