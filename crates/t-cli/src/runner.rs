use std::{
    env, fs,
    path::{Path, PathBuf},
};

use t_binding::{JSEngine, LuaEngine, ScriptEngine};
use t_config::Config;

pub struct Runner {
    engine: Box<dyn ScriptEngine>,
    content: String,
    config: Config,
}

impl Runner {
    pub fn new(file: impl AsRef<Path>, config: Config) -> Self {
        let content = fs::read_to_string(&file).unwrap();
        let ext = file.as_ref().extension().and_then(|x| x.to_str()).unwrap();
        let e = match ext {
            "js" => JSEngine::new(),
            "lua" => LuaEngine::new(),
            _ => unimplemented!(),
        };
        let res = Self {
            engine: e,
            content,
            config,
        };
        res
    }

    pub fn run(&mut self) {
        self.engine.run(self.content.as_str());
    }

    pub fn dump_log(&mut self) {
        let log_dir = self
            .config
            .log_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap().to_str().unwrap().to_string());
        let log_dir = Path::new(&log_dir);

        if self.config.console.ssh.enable {
            let ssh = crate::get_mut_global_ssh();
            let mut log_path = PathBuf::new();
            log_path.push(&log_dir);
            log_path.push("ssh_full_log.txt");
            fs::write(log_path, ssh.dump_history()).unwrap();
        }

        if self.config.console.serial.enable {
            let serial = crate::get_mut_global_serial();
            let mut log_path = PathBuf::new();
            log_path.push(&log_dir);
            log_path.push("serial_full_log.txt");
            fs::write(log_path, serial.dump_history()).unwrap();
        }
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_runner() {}
}
