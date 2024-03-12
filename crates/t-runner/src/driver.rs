use crate::error::DriverError;
use crate::server::Server;
use crate::ServerBuilder;
use image::ImageBuffer;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time;
use std::time::UNIX_EPOCH;
use t_config::Config;
use t_console::RectContainer;
use t_console::SSH;

pub struct Driver {
    pub config: Config,
    server: Option<Server>,
    stop_tx: mpsc::Sender<()>,
}

type Result<T> = std::result::Result<T, DriverError>;

impl Driver {
    pub fn new(config: Config) -> Result<Self> {
        let mut builder = ServerBuilder::new(config.clone());

        let _vnc = &config.console.vnc;
        if _vnc.enable {
            if let Some(ref dir) = _vnc.screenshot_dir {
                let (screenshot_tx, screenshot_rx) = mpsc::channel();
                builder = builder.with_vnc_screenshot_subscriber(screenshot_tx);
                Self::save_screenshots(screenshot_rx, dir);
            }
        }
        let (server, stop_tx) = builder.build().map_err(DriverError::ConsoleError)?;

        Ok(Self {
            config,
            server: Some(server),
            stop_tx,
        })
    }

    fn save_screenshots(screenshot_rx: Receiver<RectContainer<[u8; 3]>>, dir: &str) {
        let path: PathBuf = PathBuf::from(dir);
        thread::spawn(move || {
            let mut path = path;
            while let Ok(screen) = screenshot_rx.recv() {
                let p = ImageBuffer::from(screen);

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

    pub fn start(&mut self) -> &mut Self {
        if let Some(server) = self.server.take() {
            server.start();
        }
        self
    }

    pub fn reconnect(&mut self) -> &mut Self {
        // TODO
        self
    }

    pub fn stop(&mut self) -> &mut Self {
        if self.stop_tx.send(()).is_err() {
            tracing::error!("stop server failed");
        }
        self
    }

    pub fn new_ssh(&mut self) -> Result<SSH> {
        SSH::new(self.config.console.ssh.clone()).map_err(DriverError::ConsoleError)
    }
}
