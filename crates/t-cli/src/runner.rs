use std::{
    fs,
    path::{Path, PathBuf},
};

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

    pub fn dump_log(&mut self, dir: impl AsRef<Path>) {
        let ssh = crate::get_mut_global_ssh();
        let mut log_path = PathBuf::new();
        log_path.push(dir);
        log_path.push("ssh_full_log.txt");
        fs::write(log_path, ssh.history()).unwrap();
    }
}

#[cfg(test)]
mod test {
    use t_binding::JSEngine;

    #[test]
    fn test_runner() {}
}
