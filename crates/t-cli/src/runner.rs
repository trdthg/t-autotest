use parking_lot::Mutex;
use std::{
    fs::{self},
    ops::Add,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{self, Duration},
};
use t_binding::{JSEngine, LuaEngine, MsgReq, MsgRes, MsgResError, ScriptEngine};
use t_config::{Config, Console, ConsoleSSHAuthType};
use t_console::{SSHAuthAuth, SSHClient, SerialClient, VNCClient, VNCEventReq, VNCEventRes};
use tracing::{error, info, warn};

use crate::needle::NeedleManager;

#[derive(Clone)]
struct AMOption<T> {
    inner: Arc<Mutex<Option<T>>>,
}

impl<T> AMOption<T> {
    fn new(val: Option<T>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(val)),
        }
    }

    fn map_mut<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        self.inner.lock().as_mut().map(f)
    }

    fn map_ref<R, F>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&T) -> R,
    {
        self.inner.lock().as_ref().map(f)
    }

    fn is_some(&self) -> bool {
        self.inner.lock().is_some()
    }
}

pub struct Runner {
    // done_tx: mpsc::Sender<()>,
    // content: String,
    config: Config,

    done_rx: mpsc::Receiver<()>,
    start_tx: mpsc::Sender<()>,

    rx: Receiver<(MsgReq, Sender<MsgRes>)>,

    ssh_client: AMOption<SSHClient>,
    serial_client: AMOption<SerialClient>,
    vnc_client: AMOption<VNCClient>,
}

impl Runner {
    pub fn new(file: impl AsRef<Path>, config: Config) -> Self {
        let content = fs::read_to_string(&file).unwrap();

        let Config {
            console:
                Console {
                    ssh: _ssh,
                    serial: _serial,
                    vnc: _vnc,
                },
            log_dir: _,
            needle_dir: _,
            env: _,
        } = config.clone();

        info!(msg = "init...");

        let serial_client = if _serial.enable {
            info!(msg = "init serial...");

            let auth = if _serial.auto_login {
                Some((
                    _serial.username.clone().unwrap(),
                    _serial.password.clone().unwrap(),
                ))
            } else {
                None
            };

            let serial_console =
                SerialClient::connect(_serial.serial_file.clone(), _serial.bund_rate, auth)
                    .expect("init serial connection failed");
            info!(msg = "init serial done");
            Some(serial_console)
        } else {
            None
        };

        let ssh_client = if _ssh.enable {
            info!(msg = "init ssh...");
            let auth = match _ssh.auth.r#type {
                ConsoleSSHAuthType::PrivateKey => SSHAuthAuth::PrivateKey(
                    _ssh.auth.private_key.clone().unwrap_or(
                        home::home_dir()
                            .map(|mut x| {
                                x.push(Path::new(".ssh/id_rsa"));
                                x.display().to_string()
                            })
                            .unwrap(),
                    ),
                ),
                ConsoleSSHAuthType::Password => {
                    SSHAuthAuth::Password(_ssh.auth.password.clone().unwrap())
                }
            };
            let ssh_client = SSHClient::connect(
                _ssh.timeout,
                auth,
                _ssh.username.clone(),
                format!("{}:{}", _ssh.host, _ssh.port),
            )
            .expect("init ssh connection failed");
            info!(msg = "init ssh done");
            Some(ssh_client)
        } else {
            None
        };

        let vnc_client = if _vnc.enable {
            info!(msg = "init vnc...");
            let vnc_client = VNCClient::connect(
                format!("{}:{}", _vnc.host, _vnc.port),
                _vnc.password.clone(),
                _vnc.screenshot_dir,
            )
            .expect("init vnc connection failed");
            info!(msg = "init vnc done");
            Some(vnc_client)
        } else {
            None
        };

        let (tx, rx) = mpsc::channel();
        t_binding::init(tx);

        let (done_tx, done_rx) = mpsc::channel();
        let (start_tx, start_rx) = mpsc::channel();

        let ext = file
            .as_ref()
            .extension()
            .unwrap()
            .to_string_lossy()
            .to_string();

        thread::spawn(move || {
            start_rx.recv().unwrap();

            let mut e: Box<dyn ScriptEngine> = match ext.as_str() {
                "js" => Box::new(JSEngine::new()),
                "lua" => Box::new(LuaEngine::new()),
                _ => unimplemented!(),
            };
            e.run(content.as_str());
            if let Err(e) = done_tx.send(()) {
                error!(msg = "send to done channel should not failed", reason = ?e);
                panic!();
            }
        });

        let res = Self {
            config,

            done_rx,

            start_tx,

            rx,

            ssh_client: AMOption::new(ssh_client),
            serial_client: AMOption::new(serial_client),
            vnc_client: AMOption::new(vnc_client),
        };

        res
    }

    pub fn run(&self) {
        self.start_tx.send(()).unwrap();
        info!(msg = "start msg handler thread");

        let config = self.config.clone();
        let ssh_client = &self.ssh_client;
        let serial_client = &self.serial_client;
        let vnc_client = &self.vnc_client;

        loop {
            // stop on receive done signal
            if let Ok(()) = self.done_rx.try_recv() {
                info!(msg = "runner handler thread stopped");
                break;
            }

            // handle msg
            let res = self.rx.recv_timeout(Duration::from_secs(10));
            if res.is_err() {
                continue;
            }
            let (req, tx) = res.unwrap();
            info!(msg = "recv script engine request", req = ?req);
            let res = match req {
                // common
                MsgReq::GetConfig { key } => MsgRes::Value(
                    config
                        .env
                        .get(&key)
                        .map(|v| v.to_owned())
                        .unwrap_or(toml::Value::String("".to_string())),
                ),
                MsgReq::ScriptRun { cmd, timeout } => {
                    if ssh_client.is_some() {
                        let res = ssh_client
                            .map_mut(|c| c.exec_seperate(&cmd))
                            .unwrap_or(Ok((-1, "no ssh".to_string())))
                            .map_err(|_| MsgResError::Timeout);
                        MsgRes::ScriptRun(res)
                    } else if serial_client.is_some() {
                        let res = serial_client
                            .map_mut(|c| c.exec_global(timeout, &cmd))
                            .unwrap_or(Ok((-1, "no serial".to_string())))
                            .map_err(|_| MsgResError::Timeout);
                        MsgRes::ScriptRun(res)
                    } else {
                        MsgRes::ScriptRun(Err(MsgResError::Timeout))
                    }
                }
                // ssh
                MsgReq::SSHScriptRunSeperate { cmd, timeout: _ } => {
                    let client = ssh_client;
                    let res = client
                        .map_mut(|c| c.exec_seperate(&cmd))
                        .unwrap_or(Ok((-1, "no ssh".to_string())))
                        .map_err(|_| MsgResError::Timeout);
                    MsgRes::ScriptRun(res)
                }
                MsgReq::ScriptRunGlobal {
                    cmd,
                    console,
                    timeout,
                } => {
                    let res = match console {
                        Some(t_binding::TextConsole::Serial) | None if serial_client.is_some() => {
                            let res = serial_client
                                .map_mut(|c| c.exec_global(timeout, &cmd))
                                .unwrap_or(Ok((1, "no serial".to_string())))
                                .map_err(|_| MsgResError::Timeout);
                            res
                        }
                        Some(t_binding::TextConsole::SSH) | None if ssh_client.is_some() => {
                            let res = ssh_client
                                .map_mut(|c| c.exec_global(timeout, &cmd))
                                .unwrap_or(Ok((-1, "no ssh".to_string())))
                                .map_err(|_| MsgResError::Timeout);
                            res
                        }
                        _ => Err(MsgResError::Timeout),
                    };
                    MsgRes::ScriptRun(res)
                }
                MsgReq::WriteStringGlobal { console, s } => {
                    match console {
                        Some(t_binding::TextConsole::SSH) => {
                            let client = ssh_client;
                            client
                                .map_mut(|c| c.write_string(&s))
                                .expect("no ssh")
                                .map_err(|_| MsgResError::Timeout)
                                .unwrap();
                        }
                        Some(t_binding::TextConsole::Serial) => {
                            let client = serial_client;
                            client
                                .map_mut(|c| c.write_string(&s))
                                .expect("no serial")
                                .map_err(|_| MsgResError::Timeout)
                                .unwrap();
                        }
                        None => {}
                    }
                    MsgRes::Done
                }
                MsgReq::AssertScreen {
                    tag,
                    threshold: _,
                    timeout,
                } => {
                    let nmg = NeedleManager::new(&config.needle_dir);
                    let res = {
                        let deadline = time::Instant::now().add(timeout);
                        loop {
                            let (tx, rx) = mpsc::channel();

                            vnc_client
                                .map_ref(|c| c.event_tx.send((VNCEventReq::Dump, tx)))
                                .expect("no vnc")
                                .unwrap();

                            match rx.recv_deadline(deadline) {
                                Ok(VNCEventRes::Screen(s)) => {
                                    let res = nmg.cmp_by_tag(&s, &tag);
                                    if !res {
                                        warn!(msg = "match failed", tag = tag);
                                        continue;
                                    }
                                    info!(msg = "match success", tag = tag);
                                    break Ok(res);
                                }
                                Ok(res) => {
                                    warn!(msg = "invalid msg type", v = ?res);
                                }
                                Err(_e) => break Err(MsgResError::Timeout),
                            }
                        }
                    };
                    MsgRes::AssertScreen {
                        similarity: 0,
                        ok: res.is_ok(),
                    }
                }
                MsgReq::MouseMove { x, y } => {
                    let client = vnc_client;
                    let (tx, rx) = mpsc::channel();
                    client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::MouseMove(x, y), tx)))
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
                MsgReq::MouseClick => {
                    let (tx, rx) = mpsc::channel();
                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::MoveDown, tx)))
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));

                    let (tx, rx) = mpsc::channel();

                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::MoveUp, tx)))
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
                MsgReq::MouseHide => {
                    let (tx, rx) = mpsc::channel();
                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::MouseHide, tx)))
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
            };
            info!(msg = format!("sending res: {:?}", res));
            if let Err(e) = tx.send(res) {
                info!(msg = "script engine receiver closed", reason = ?e);
                break;
            }
        }
    }

    pub fn dump_log(&mut self) {
        let log_dir = Path::new(&self.config.log_dir);

        if self.config.console.ssh.enable {
            info!(msg = "collecting ssh log...");
            let mut log_path = PathBuf::new();
            log_path.push(log_dir);
            log_path.push("ssh_full_log.txt");
            let history = self.ssh_client.map_ref(|c| c.dump_history()).unwrap();
            fs::write(log_path, history).unwrap();
            info!(msg = "collecting ssh log done");
        }

        if self.config.console.serial.enable {
            info!(msg = "collecting serialport log...");
            let history = self.serial_client.map_ref(|c| c.dump_history()).unwrap();
            let mut log_path = PathBuf::new();
            log_path.push(log_dir);
            log_path.push("serial_full_log.txt");
            fs::write(log_path, history).unwrap();
            info!(msg = "collecting serialport log done");
        }
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_runner() {}
}
