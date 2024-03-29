use crate::engine::Engine;
use crate::engine::EngineClient;
use crate::error::DriverError;
use crate::server::Server;
use crate::ServerBuilder;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time;
use std::time::UNIX_EPOCH;
use t_config::Config;
use t_console::PNG;
use t_console::SSH;

pub struct DriverForScript {
    pub config: Config,
    server: Option<Server>,
    server_stop_tx: mpsc::Sender<()>,

    engine: Option<Engine>,
    engine_client: Option<EngineClient>,
}

type Result<T> = std::result::Result<T, DriverError>;

impl DriverForScript {
    fn new(config: Config) -> Result<Self> {
        let mut builder = ServerBuilder::new(config.clone());

        if let Some(vnc) = config.vnc.as_ref() {
            if let Some(ref dir) = vnc.screenshot_dir {
                let (screenshot_tx, screenshot_rx) = mpsc::channel();
                builder = builder.with_vnc_screenshot_subscriber(screenshot_tx);
                Self::save_screenshots(screenshot_rx, dir.clone());
            }
        }
        let (s, c) = builder.build().map_err(DriverError::ConsoleError)?;

        Ok(Self {
            config,
            server: Some(s),
            server_stop_tx: c,
            engine: None,
            engine_client: None,
        })
    }

    fn save_screenshots(screenshot_rx: Receiver<PNG>, dir: PathBuf) {
        let path: PathBuf = dir;
        thread::spawn(move || {
            let mut path = path;
            while let Ok(screen) = screenshot_rx.recv() {
                let p = screen.into_img();

                let image_name = format!(
                    "output-{}.png",
                    time::SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                );
                path.push(&image_name);
                p.save(&path).unwrap();
                path.pop();

                path.push("latest.png");
                p.save(&path).unwrap();
                path.pop();
            }
        });
    }

    pub fn new_with_engine(config: Config, ext: String) -> Result<Self> {
        let mut res = Self::new(config)?;
        let (engine, enginec) = Engine::new(ext.as_str());
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
        if let Some(s) = self.server.take() {
            s.start();
            // recover server after stop
        }
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
        if self.server_stop_tx.send(()).is_err() {
            tracing::error!("stop server failed");
        }

        self
    }

    pub fn run_file(&mut self, script: String) -> &mut Self {
        if let Some(c) = self.engine_client.as_mut() {
            c.run_file(script.as_str());
        }
        self
    }

    pub fn new_ssh(&mut self) -> Result<SSH> {
        if let Some(ssh) = self.config.ssh.clone() {
            SSH::new(ssh.clone()).map_err(DriverError::ConsoleError)
        } else {
            Err(DriverError::ConsoleError(t_console::ConsoleError::Timeout))
        }
    }
}
