use crate::engine::EngineClient;
use crate::server::Server;
use crate::{engine::Engine, server::ServerClient};
use std::fmt::Display;
use std::sync::mpsc;
use std::thread;
use t_config::Config;
use t_console::{ConsoleError, SSH};

pub struct Driver {
    pub config: Config,
    s: Option<Server>,
    s_tx: mpsc::Sender<Server>,
    s_rx: mpsc::Receiver<Server>,
    c: ServerClient,
    e: Option<Engine>,
    ec: Option<EngineClient>,
}

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

type Result<T> = std::result::Result<T, DriverError>;

impl Driver {
    pub fn new(config: Config) -> Result<Self> {
        let (s, c) = Server::new(config.clone()).map_err(DriverError::ConsoleError)?;
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            config,
            s: Some(s),
            s_rx: rx,
            s_tx: tx,
            c,
            e: None,
            ec: None,
        })
    }

    pub fn new_with_engine(config: Config, ext: String) -> Result<Self> {
        let mut res = Self::new(config)?;
        let (engine, enginec) = Engine::new(ext.as_str());
        res.e = Some(engine);
        res.ec = Some(enginec);
        Ok(res)
    }

    pub fn start(&mut self) -> &mut Self {
        // spawn script engine if some
        if let Some(mut e) = self.e.take() {
            thread::spawn(move || {
                e.start();
            });
        }

        // spawn server non-blocking
        let s = self.s.take();
        let tx = self.s_tx.clone();
        thread::spawn(move || {
            if let Some(s) = s {
                s.start();
                // recover server after stop
                tx.send(s).unwrap();
            }
        });
        self
    }

    pub fn reconnect(&mut self) -> &mut Self {
        // TODO
        self
    }

    pub fn stop(&mut self) -> &mut Self {
        // stop script engine if exists
        if let Some(c) = self.ec.as_mut() {
            c.stop();
        }
        // stop api handle loop
        self.c.stop();

        let server = self.s_rx.recv().unwrap();
        server.dump_log();
        server.stop();

        self
    }

    pub fn run_file(&mut self, script: String) -> &mut Self {
        if let Some(c) = self.ec.as_mut() {
            c.run_file(script.as_str());
        }
        self
    }

    pub fn new_ssh(&mut self) -> Result<SSH> {
        SSH::new(self.config.console.ssh.clone()).map_err(DriverError::ConsoleError)
    }
}
