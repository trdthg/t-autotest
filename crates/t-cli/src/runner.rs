use std::{fs, path::Path};

use t_binding::{JSEngine, LuaEngine, ScriptEngine};

pub struct Runner {
    engine: Box<dyn ScriptEngine>,
    content: String,
}

impl Runner {
    pub fn new(file: impl AsRef<Path>) -> Self {
        let content = fs::read_to_string(&file).unwrap();
        let ext = file.as_ref().extension().and_then(|x| x.to_str()).unwrap();
        let e = match ext {
            "js" => JSEngine::new(),
            "lua" => LuaEngine::new(),
            _ => unimplemented!(),
        };
        let res = Self { engine: e, content };
        res
    }

    pub fn run(&mut self) {
        self.engine.run(self.content.as_str());
    }
}

#[cfg(test)]
mod test {
    use t_binding::JSEngine;

    #[test]
    fn test_runner() {}
}
