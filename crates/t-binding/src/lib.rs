mod api;
mod engine;
mod msg;

use api::Callback;
pub use engine::{JSEngine, LuaEngine};

pub enum EngineError {}

pub trait ScriptEngine {}

pub struct Runner {
    engine: Box<dyn ScriptEngine>,
}

impl Runner {
    pub fn new(engine: Box<dyn ScriptEngine>) -> Self {
        let res = Self { engine };
        res
    }

    pub fn register_fn<Args>(&mut self, f: impl Callback<Args>) {}

    pub fn run(&mut self) {}
}

#[cfg(test)]
mod test {
    use crate::{JSEngine, Runner};

    #[test]
    fn test_runner() {}
}
