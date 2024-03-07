use crate::engine::EngineClient;
use crate::server::Server;
use crate::{engine::Engine, server::ServerClient};
use std::sync::mpsc;
use std::thread;
use t_config::Config;
use t_console::SSH;

pub struct Driver {
    pub config: Config,
    s: Option<Server>,
    s_tx: mpsc::Sender<Server>,
    s_rx: mpsc::Receiver<Server>,
    c: ServerClient,
    e: Option<Engine>,
    ec: Option<EngineClient>,
}

impl Driver {
    pub fn new(config: Config) -> Self {
        let (s, c) = Server::new(config.clone());
        let (tx, rx) = mpsc::channel();
        Self {
            config,
            s: Some(s),
            s_rx: rx,
            s_tx: tx,
            c,
            e: None,
            ec: None,
        }
    }

    pub fn new_with_engine(config: Config, ext: String) -> Self {
        let mut res = Self::new(config);
        let (engine, enginec) = Engine::new(ext.as_str());
        res.e = Some(engine);
        res.ec = Some(enginec);
        res
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
        if let Some(c) = self.ec.as_mut() {
            c.stop();
        }
        self.c.stop();
        let server = self.s_rx.recv().unwrap();
        self.s = Some(server);
        self.dump_log();
        self
    }

    pub fn run_script(&mut self, script: String) -> &mut Self {
        if let Some(c) = self.ec.as_mut() {
            c.run(script.as_str());
        }
        self
    }

    fn dump_log(&mut self) -> &mut Self {
        if let Some(s) = self.s.as_ref() {
            s.dump_log();
        }
        self
    }

    pub fn new_ssh(&mut self) -> SSH {
        SSH::new(self.config.console.ssh.clone())
    }
}
