use crate::engine::Engine;
use crate::engine::EngineClient;
use crate::error::DriverError;
use crate::Driver;
use std::thread;
use t_config::Config;
use t_console::SSH;

pub struct DriverForScript {
    driver: Driver,
    engine: Option<Engine>,
    engine_client: Option<EngineClient>,
}

type Result<T> = std::result::Result<T, DriverError>;

impl DriverForScript {
    fn new(config: Config) -> Result<Self> {
        let driver = Driver::new(Some(config.clone()))?;

        Ok(Self {
            driver,
            engine: None,
            engine_client: None,
        })
    }

    pub fn new_with_engine(config: Config, ext: &str) -> Result<Self> {
        let mut res = Self::new(config)?;
        let (engine, enginec) = Engine::new(ext, res.driver.msg_tx.clone());
        res.engine = Some(engine);
        res.engine_client = Some(enginec);
        Ok(res)
    }

    pub fn start(&mut self) -> &mut Self {
        // spawn script engine if some
        if let Some(mut e) = self.engine.take() {
            thread::spawn(move || {
                e.start();
            });
        }

        // spawn server non-blocking
        self.driver.start();

        self
    }

    pub fn reconnect(&mut self) -> &mut Self {
        // TODO
        self
    }

    pub fn stop(&mut self) -> &mut Self {
        // stop script engine if exists
        if let Some(c) = self.engine_client.as_mut() {
            c.stop();
        }

        // stop api handle loop. TODO: ensure server is stopped
        self.driver.stop();

        self
    }

    pub fn run_file(&mut self, script: String) -> &mut Self {
        if let Some(c) = self.engine_client.as_mut() {
            c.run_file(script.as_str());
        }
        self
    }

    pub fn new_ssh(&mut self) -> Result<SSH> {
        if let Some(ssh) = self.driver.config.as_ref().and_then(|c| c.ssh.clone()) {
            SSH::new(ssh.clone()).map_err(DriverError::ConsoleError)
        } else {
            Err(DriverError::ConsoleError(t_console::ConsoleError::Timeout))
        }
    }
}
