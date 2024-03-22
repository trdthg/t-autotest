use crate::error::DriverError;
use crate::server::Server;
use crate::ServerBuilder;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;
use std::time;
use std::time::UNIX_EPOCH;
use t_config::Config;
use t_console::PNG;
use t_console::SSH;
use tracing::warn;

pub struct Driver {
    pub config: Config,
    server: Option<Server>,
    stop_tx: mpsc::Sender<()>,
    screenshot_save_done_rx: Option<mpsc::Receiver<()>>,
}

type Result<T> = std::result::Result<T, DriverError>;

impl Driver {
    pub fn new(config: Config) -> Result<Self> {
        let mut builder = ServerBuilder::new(config.clone());

        let _vnc = &config.console.vnc;
        let mut stop_rx = None::<Receiver<()>>;
        if _vnc.enable {
            if let Some(ref dir) = _vnc.screenshot_dir {
                let (screenshot_tx, screenshot_rx) = mpsc::channel();
                let (screenshot_tx2, screenshot_rx2) = mpsc::channel();
                builder = builder
                    .with_vnc_screenshot_subscriber(screenshot_tx)
                    .with_latest_vnc_screenshot_subscriber(screenshot_tx2);
                let (done_tx, donw_rx) = mpsc::channel();
                Self::save_screenshots(screenshot_rx, dir, done_tx);
                stop_rx = Some(donw_rx);

                let latest_screenshot_path = PathBuf::from_iter(vec![dir, "latest.png"]);
                thread::spawn(move || {
                    while let Ok(screen) = screenshot_rx2.recv() {
                        let p = screen.into_img();
                        if let Err(e) = p.save(&latest_screenshot_path) {
                            warn!(msg="screenshot save failed", reason=?e);
                        }
                    }
                });
            }
        }
        let (server, stop_tx) = builder.build().map_err(DriverError::ConsoleError)?;

        let res = Self {
            config,
            server: Some(server),
            stop_tx,
            screenshot_save_done_rx: stop_rx,
        };
        Ok(res)
    }

    fn save_screenshots(screenshot_rx: Receiver<PNG>, dir: &str, stop_tx: Sender<()>) {
        let path: PathBuf = PathBuf::from(dir);
        thread::spawn(move || {
            let mut path = path;
            let mut i = 0;
            while let Ok(screen) = screenshot_rx.recv() {
                i += 1;
                let p = screen.into_img();

                let image_name = format!(
                    "output-{}-{}.png",
                    i,
                    time::SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                );
                path.push(&image_name);
                if let Err(e) = p.save(&path) {
                    warn!(msg="screenshot save failed", reason=?e);
                }
                path.pop();
            }
            let _ = stop_tx.send(());
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
        if let Some(rx) = &self.screenshot_save_done_rx {
            let _ = rx.recv();
        }
        self
    }

    pub fn new_ssh(&mut self) -> Result<SSH> {
        SSH::new(self.config.console.ssh.clone()).map_err(DriverError::ConsoleError)
    }
}
