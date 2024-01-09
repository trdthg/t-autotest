use std::{
    fs::{self},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use t_binding::{JSEngine, LuaEngine, MsgReq, MsgRes, ScriptEngine};
use t_config::{Config, Console, ConsoleSSHAuthType};
use t_console::{SSHAuthAuth, SSHClient, SerialClient, VNCClient, VNCEventReq, VNCEventRes};
use tracing::{debug, info, warn};

use crate::needle::NeedleManager;

pub struct Runner {
    engine: Box<dyn ScriptEngine>,
    content: String,
    config: Config,

    ssh_client: Option<Arc<Mutex<SSHClient>>>,
    serial_client: Option<Arc<Mutex<SerialClient>>>,
    vnc_client: Option<Arc<Mutex<VNCClient>>>,
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
        } = config.clone();

        info!("init...");

        let ssh_client = if _ssh.enable {
            info!("init ssh...");
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
                    SSHAuthAuth::PrivateKey(_ssh.auth.password.clone().unwrap())
                }
            };
            let ssh_client = SSHClient::connect(
                auth,
                _ssh.username.clone(),
                format!("{}:{}", _ssh.host, _ssh.port),
            )
            .unwrap();
            info!("init ssh done");
            Some(Arc::new(Mutex::new(ssh_client)))
        } else {
            None
        };

        let serial_client = if _serial.enable {
            info!("init serial...");

            let auth = if _serial.auto_login {
                Some((
                    _serial.username.clone().unwrap(),
                    _serial.password.clone().unwrap(),
                ))
            } else {
                None
            };

            let serial_console = SerialClient::connect(
                _serial.serial_file.clone(),
                _serial.bund_rate,
                Duration::from_secs(0),
                auth,
            )
            .unwrap();
            info!("init serial done");
            Some(Arc::new(Mutex::new(serial_console)))
        } else {
            None
        };

        let vnc_client = if _vnc.enable {
            info!("init vnc...");
            let vnc_client = VNCClient::connect(
                format!("{}:{}", _vnc.host, _vnc.port),
                _vnc.password.clone(),
            )
            .unwrap();
            info!("init vnc done");
            Some(Arc::new(Mutex::new(vnc_client)))
        } else {
            None
        };

        let (tx, rx) = mpsc::channel();
        t_binding::init(tx);

        let res = Self {
            engine: e,
            content,
            config,
            ssh_client,
            serial_client,
            vnc_client,
        };

        res.spawn_handler(rx);

        res
    }

    fn spawn_handler(&self, rx: Receiver<(MsgReq, Sender<MsgRes>)>) {
        info!("start msg handler thread");

        let config = self.config.clone();
        let ssh_client = self.ssh_client.clone();
        let serial_client = self.serial_client.clone();
        let vnc_client = self.vnc_client.clone();

        thread::spawn(move || {
            while let Ok((msg, tx)) = rx.recv() {
                info!("recv msg: {:#?}", msg);
                let res = match msg {
                    MsgReq::AssertScreen {
                        tag,
                        threshold,
                        timeout,
                    } => {
                        let client = vnc_client.clone().unwrap();
                        let nmg = NeedleManager::new(&config.needle_dir);
                        let res = t_util::run_with_timeout(
                            move || loop {
                                let (tx, rx) = mpsc::channel();
                                client
                                    .lock()
                                    .unwrap()
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
                        )
                        .unwrap();
                        MsgRes::AssertScreen {
                            similarity: 0,
                            ok: res,
                        }
                    }
                    MsgReq::AssertScriptRunSshSeperate { cmd, timeout } => {
                        let client = ssh_client.clone().unwrap();
                        let res = t_util::run_with_timeout(
                            move || {
                                let mut c = client.lock().unwrap();
                                c.exec_seperate(&cmd).unwrap()
                            },
                            timeout,
                        )
                        .unwrap();
                        MsgRes::AssertScriptRunSshSeperate { res }
                    }
                    MsgReq::AssertScriptRunSshGlobal { cmd, timeout } => {
                        let client = ssh_client.clone().unwrap();
                        let res = t_util::run_with_timeout(
                            move || {
                                let mut c = client.lock().unwrap();
                                c.exec_global(&cmd).unwrap()
                            },
                            timeout,
                        )
                        .unwrap();
                        MsgRes::AssertScriptRunSshGlobal { res }
                    }
                    MsgReq::AssertScriptRunSerialGlobal { cmd, timeout } => {
                        let client = serial_client.clone().unwrap();
                        let res = t_util::run_with_timeout(
                            move || {
                                client
                                    .lock()
                                    .unwrap()
                                    .exec_global(&cmd)
                                    .expect("serial connection broken")
                            },
                            timeout,
                        )
                        .unwrap();
                        MsgRes::AssertScriptRunSerialGlobal { res }
                    }
                };
                info!("send res: {:#?}", res);
                tx.send(res).unwrap();
            }
        });
        info!("init done");
    }

    pub fn run(&mut self) {
        self.engine.run(self.content.as_str());
    }

    pub fn dump_log(&mut self) {
        let log_dir = Path::new(&self.config.log_dir);

        if self.config.console.ssh.enable {
            let ssh = self.ssh_client.clone().unwrap();
            let mut log_path = PathBuf::new();
            log_path.push(&log_dir);
            log_path.push("ssh_full_log.txt");
            fs::write(log_path, ssh.lock().unwrap().dump_history()).unwrap();
        }

        if self.config.console.serial.enable {
            let serial = self.serial_client.clone().unwrap();
            let mut log_path = PathBuf::new();
            log_path.push(&log_dir);
            log_path.push("serial_full_log.txt");
            fs::write(log_path, serial.lock().unwrap().dump_history()).unwrap();
        }
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_runner() {}
}
