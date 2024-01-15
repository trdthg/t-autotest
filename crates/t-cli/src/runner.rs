use parking_lot::Mutex;
use std::{
    fs::{self},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::Duration,
};
use t_binding::{JSEngine, LuaEngine, MsgReq, MsgRes, MsgResError, ScriptEngine};
use t_config::{Config, Console, ConsoleSSHAuthType};
use t_console::{SSHAuthAuth, SSHClient, SerialClient, VNCClient, VNCEventReq, VNCEventRes};
use tracing::{error, info, warn};

use crate::needle::NeedleManager;

pub struct Runner {
    engine: Box<dyn ScriptEngine>,
    content: String,
    config: Config,

    done_ch: mpsc::Sender<()>,
    ssh_client: Arc<Mutex<Option<SSHClient>>>,
    serial_client: Arc<Mutex<Option<SerialClient>>>,
    vnc_client: Arc<Mutex<Option<VNCClient>>>,
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
                    .unwrap();
            info!(msg = "init serial done");
            Some(serial_console)
        } else {
            None
        };
        let serial_client = Arc::new(Mutex::new(serial_client));

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
            .unwrap();
            info!(msg = "init ssh done");
            Some(ssh_client)
        } else {
            None
        };
        let ssh_client = Arc::new(Mutex::new(ssh_client));

        let vnc_client = if _vnc.enable {
            info!(msg = "init vnc...");
            let vnc_client = VNCClient::connect(
                format!("{}:{}", _vnc.host, _vnc.port),
                _vnc.password.clone(),
            )
            .unwrap();
            info!(msg = "init vnc done");
            Some(vnc_client)
        } else {
            None
        };
        let vnc_client = Arc::new(Mutex::new(vnc_client));

        let (tx, rx) = mpsc::channel();
        t_binding::init(tx);

        let (done_tx, done_rx) = mpsc::channel();

        let res = Self {
            engine: e,
            content,
            config,
            done_ch: done_tx,
            ssh_client,
            serial_client,
            vnc_client,
        };

        res.spawn_handler(rx, done_rx);

        res
    }

    fn spawn_handler(&self, rx: Receiver<(MsgReq, Sender<MsgRes>)>, done_rx: Receiver<()>) {
        info!(msg = "start msg handler thread");

        let config = self.config.clone();
        let ssh_client = self.ssh_client.clone();
        let serial_client = self.serial_client.clone();
        let vnc_client = self.vnc_client.clone();

        thread::spawn(move || loop {
            // stop on receive done signal
            if let Ok(()) = done_rx.try_recv() {
                info!(msg = "runner handler thread stopped");
                break;
            }

            let res = rx.recv_timeout(Duration::from_secs(10));
            if res.is_err() {
                continue;
            }
            let (req, tx) = res.unwrap();
            info!(msg = "recv script engine request", req = ?req);
            let res = match req {
                MsgReq::GetConfig { key } => MsgRes::Value(
                    config
                        .env
                        .get(&key)
                        .map(|v| v.to_owned())
                        .unwrap_or(toml::Value::String("".to_string())),
                ),
                MsgReq::SSHAssertScriptRunSeperate { cmd, timeout } => {
                    let client = ssh_client.clone();
                    let res = client
                        .lock()
                        .as_mut()
                        .expect("no ssh")
                        .exec_seperate(&cmd)
                        .map_err(|_| MsgResError::Timeout);
                    MsgRes::SSHAssertScriptRunSeperate(res)
                }
                MsgReq::SSHAssertScriptRunGlobal { cmd, timeout } => {
                    let client = ssh_client.clone();
                    let res = client
                        .lock()
                        .as_mut()
                        .expect("no ssh")
                        .exec_global(timeout, &cmd)
                        .map_err(|_| MsgResError::Timeout);
                    MsgRes::SSHAssertScriptRunGlobal(res)
                }
                MsgReq::SSHWriteStringGlobal { s } => {
                    let client = ssh_client.clone();
                    let res = client
                        .lock()
                        .as_mut()
                        .expect("no ssh")
                        .write_string(&s)
                        .map_err(|_| MsgResError::Timeout);
                    MsgRes::Done
                }
                MsgReq::SerialAssertScriptRunGlobal { cmd, timeout } => {
                    let client = serial_client.clone();
                    let res = client
                        .try_lock()
                        .unwrap()
                        .as_mut()
                        .expect("no serial")
                        .exec_global(timeout, &cmd)
                        .map_err(|_| MsgResError::Timeout);
                    MsgRes::SerialAssertScriptRunGlobal(res)
                }
                MsgReq::SerialWriteStringGlobal { s } => {
                    let client = serial_client.clone();
                    client
                        .try_lock()
                        .unwrap()
                        .as_ref()
                        .expect("no serial")
                        .write_string(&s)
                        .unwrap();
                    MsgRes::Done
                }
                MsgReq::AssertScreen {
                    tag,
                    threshold: _,
                    timeout,
                } => {
                    let client = vnc_client.clone();
                    let nmg = NeedleManager::new(&config.needle_dir);
                    let res = t_util::run_with_timeout(
                        move || loop {
                            let (tx, rx) = mpsc::channel();
                            client
                                .lock()
                                .as_ref()
                                .expect("no vnc")
                                .event_tx
                                .send((VNCEventReq::Dump, tx))
                                .unwrap();
                            let res = if let VNCEventRes::Screen(s) = rx.recv().unwrap() {
                                nmg.cmp_by_tag(&s, &tag)
                            } else {
                                false
                            };
                            if !res {
                                warn!(msg = "match failed", tag = tag);
                                continue;
                            }
                            warn!(msg = "match success", tag = tag);
                            break res;
                        },
                        timeout,
                    );
                    MsgRes::AssertScreen {
                        similarity: 0,
                        ok: matches!(res, Ok(_)),
                    }
                }
                MsgReq::MouseMove { x, y } => {
                    let client = vnc_client.clone();
                    let (tx, rx) = mpsc::channel();
                    client
                        .lock()
                        .as_ref()
                        .expect("no vnc")
                        .event_tx
                        .send((VNCEventReq::MouseMove(x, y), tx))
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
                MsgReq::MouseClick => {
                    let client = vnc_client.clone();

                    let (tx, rx) = mpsc::channel();
                    client
                        .lock()
                        .as_ref()
                        .expect("no vnc")
                        .event_tx
                        .send((VNCEventReq::MoveDown, tx))
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));

                    let (tx, rx) = mpsc::channel();
                    client
                        .lock()
                        .as_ref()
                        .expect("no vnc")
                        .event_tx
                        .send((VNCEventReq::MoveUp, tx))
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
                MsgReq::MouseHide => {
                    let (tx, rx) = mpsc::channel();
                    vnc_client
                        .lock()
                        .as_ref()
                        .expect("no vnc")
                        .event_tx
                        .send((VNCEventReq::MouseHide, tx))
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
        });
        info!(msg = "init done");
    }

    pub fn run(&mut self) {
        self.engine.run(self.content.as_str());
        if let Err(e) = self.done_ch.send(()) {
            error!(msg = "send to done channel should not failed", reason = ?e);
            panic!();
        }
    }

    pub fn dump_log(&mut self) {
        let log_dir = Path::new(&self.config.log_dir);

        if self.config.console.ssh.enable {
            info!(msg = "collecting ssh log...");
            let mut log_path = PathBuf::new();
            log_path.push(&log_dir);
            log_path.push("ssh_full_log.txt");
            let history = self.ssh_client.lock().as_ref().unwrap().dump_history();
            fs::write(log_path, history).unwrap();
            info!(msg = "collecting ssh log done");
        }

        if self.config.console.serial.enable {
            info!(msg = "collecting serialport log...");
            let history = self
                .serial_client
                .clone()
                .lock()
                .as_ref()
                .unwrap()
                .dump_history();
            let mut log_path = PathBuf::new();
            log_path.push(&log_dir);
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
