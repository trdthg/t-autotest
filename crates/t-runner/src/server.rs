use crate::needle::NeedleManager;
use parking_lot::Mutex;
use std::{
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{self, Duration},
};
use t_binding::{MsgReq, MsgRes, MsgResError};
use t_config::{Config, Console};
use t_console::{ConsoleError, RectContainer, Serial, VNCEventReq, VNCEventRes, SSH, VNC};
use tracing::{error, info, warn};

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

pub struct Server {
    config: Config,

    msg_rx: Receiver<(MsgReq, Sender<MsgRes>)>,

    stop_rx: mpsc::Receiver<()>,

    ssh_client: AMOption<SSH>,
    serial_client: AMOption<Serial>,
    vnc_client: AMOption<VNC>,
}

pub struct ServerBuilder {
    config: Config,
    tx: Option<Sender<RectContainer<[u8; 3]>>>,
}

impl ServerBuilder {
    pub fn new(config: Config) -> Self {
        ServerBuilder { tx: None, config }
    }

    pub fn with_vnc_screenshot_subscriber(mut self, tx: Sender<RectContainer<[u8; 3]>>) -> Self {
        self.tx = Some(tx);
        self
    }

    pub fn build(self) -> Result<(Server, mpsc::Sender<()>), ConsoleError> {
        let Console {
            ssh: _ssh,
            serial: _serial,
            vnc: _vnc,
        } = self.config.console.clone();

        let serial_client = if _serial.enable {
            Some(Serial::new(_serial)?)
        } else {
            None
        };

        let ssh_client = if _ssh.enable {
            Some(SSH::new(_ssh)?)
        } else {
            None
        };

        let vnc_client = if _vnc.enable {
            let addr = format!("{}:{}", _vnc.host, _vnc.port)
                .parse()
                .map_err(|e| {
                    ConsoleError::ConnectionBroken(format!("vnc addr is not valid, {}", e))
                })?;

            let vnc_client = VNC::connect(addr, _vnc.password.clone(), self.tx)
                .map_err(|e| ConsoleError::ConnectionBroken(e.to_string()))?;

            info!(msg = "init vnc done");
            Some(vnc_client)
        } else {
            None
        };

        let (tx, msg_rx) = mpsc::channel();
        t_binding::init(tx);

        let (stop_tx, stop_rx) = mpsc::channel();

        Ok((
            Server {
                config: self.config,
                msg_rx,
                stop_rx,

                ssh_client: AMOption::new(ssh_client),
                serial_client: AMOption::new(serial_client),
                vnc_client: AMOption::new(vnc_client),
            },
            stop_tx,
        ))
    }
}

impl Server {
    pub fn start(self) {
        thread::spawn(move || {
            self.pool();
        });
    }

    fn pool(&self) {
        // start script engine if in case mode
        info!(msg = "start msg handler thread");

        let Self {
            config,
            msg_rx: _,
            stop_rx: _,
            ssh_client,
            serial_client,
            vnc_client,
        } = self;

        let _ssh_tty = ssh_client.map_ref(|c| c.tty());
        let serial_tty = serial_client.map_ref(|c| c.tty());

        loop {
            // stop on receive done signal
            if let Ok(_) = self.stop_rx.try_recv() {
                info!(msg = "runner handler thread stopped");
                break;
            }

            // handle msg
            let res = self.msg_rx.recv_timeout(Duration::from_secs(10));
            if res.is_err() {
                continue;
            }
            let (req, tx) = res.unwrap();
            info!(msg = "recv script engine request", req = ?req);
            let res = match req {
                // common
                MsgReq::GetConfig { key } => {
                    let v = config.env.get(&key).map(|v| v.to_string());
                    MsgRes::ConfigValue(v)
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
                    let res = match (console, ssh_client.is_some(), serial_client.is_some()) {
                        (None, true, true) => {
                            let serial_tty = serial_tty.as_ref().unwrap();
                            let key = nanoid::nanoid!(6);
                            ssh_client
                                .map_mut(|c| {
                                    c.write_string(&format!(
                                        "echo {} > {}; {} &> {}; echo $?{} > {}\n",
                                        &key, serial_tty, cmd, serial_tty, &key, serial_tty
                                    ))
                                })
                                .unwrap()
                                .map_err(|_| MsgResError::Timeout)
                                .unwrap();
                            let res = serial_client
                                .map_mut(|c| c.wait_string_ntimes(timeout, &key, 2))
                                .map(|v| match v {
                                    Ok(v) => {
                                        let catched =
                                            t_util::assert_capture_between(&v, &key, &key)
                                                .unwrap()
                                                .unwrap();
                                        match catched.rsplit_once('\n') {
                                            Some((res, flag)) => match flag.parse::<i32>() {
                                                Ok(flag) => Ok((flag, res.to_string())),
                                                Err(_e) => Ok((-1, catched.to_string())),
                                            },
                                            None => Ok((-1, catched)),
                                        }
                                    }
                                    Err(e) => Err(e),
                                })
                                .unwrap_or(Ok((1, "no serial".to_string())))
                                .map_err(|_e| MsgResError::Timeout);
                            res
                        }
                        // None if ssh_client.is_some() && serial_client.is_some() => {}
                        (None | Some(t_binding::TextConsole::Serial), _, true) => serial_client
                            .map_mut(|c| c.exec_global(timeout, &cmd))
                            .unwrap_or(Ok((1, "no serial".to_string())))
                            .map_err(|_| MsgResError::Timeout),
                        (None | Some(t_binding::TextConsole::SSH), true, _) => ssh_client
                            .map_mut(|c| c.exec_global(timeout, &cmd))
                            .unwrap_or(Ok((-1, "no ssh".to_string())))
                            .map_err(|_| MsgResError::Timeout),
                        _ => Err(MsgResError::String("no console supported".to_string())),
                    };
                    MsgRes::ScriptRun(res)
                }
                MsgReq::WriteStringGlobal { console, s } => {
                    if let Err(e) = match (console, ssh_client.is_some(), serial_client.is_some()) {
                        (None | Some(t_binding::TextConsole::Serial), _, true) => serial_client
                            .map_mut(|c| c.write_string(&s))
                            .expect("no serial")
                            .map_err(|_| MsgResError::Timeout),
                        (None | Some(t_binding::TextConsole::SSH), true, _) => ssh_client
                            .map_mut(|c| c.write_string(&s))
                            .expect("no ssh")
                            .map_err(|_| MsgResError::Timeout),
                        _ => Err(MsgResError::String("no console supported".to_string())),
                    } {
                        MsgRes::Error(e)
                    } else {
                        MsgRes::Done
                    }
                }
                MsgReq::WaitStringGlobal {
                    console,
                    s,
                    n,
                    timeout,
                } => {
                    if let Err(e) = match (console, ssh_client.is_some(), serial_client.is_some()) {
                        (None | Some(t_binding::TextConsole::Serial), _, true) => serial_client
                            .map_mut(|c| c.wait_string_ntimes(timeout, &s, n as usize))
                            .expect("no serial")
                            .map_err(|_| MsgResError::Timeout),
                        (None | Some(t_binding::TextConsole::SSH), true, _) => ssh_client
                            .map_mut(|c| c.wait_string_ntimes(timeout, &s, n as usize))
                            .expect("no ssh")
                            .map_err(|_| MsgResError::Timeout),
                        _ => Err(MsgResError::String("no console supported".to_string())),
                    } {
                        MsgRes::Error(e)
                    } else {
                        MsgRes::Done
                    }
                }
                MsgReq::AssertScreen {
                    tag,
                    threshold: _,
                    timeout,
                } => {
                    let nmg = NeedleManager::new(&config.needle_dir);
                    let res: Result<bool, MsgResError> = {
                        let deadline = time::Instant::now() + timeout;
                        loop {
                            let (tx, rx) = mpsc::channel();

                            vnc_client
                                .map_ref(|c| c.event_tx.send((VNCEventReq::TakeScreenShot, tx)))
                                .expect("no vnc")
                                .unwrap();

                            match rx.recv_timeout(deadline - time::Instant::now()) {
                                Ok(VNCEventRes::Screen(s)) => {
                                    let Some(res) = nmg.cmp_by_tag(&s, &tag) else {
                                        error!(msg = "Needle file not found", tag = tag);
                                        break Ok(false);
                                    };
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
                    let (tx, rx) = mpsc::channel();
                    vnc_client
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
        info!(msg = "Runner loop stopped")
    }

    pub fn stop(&self) {
        self.ssh_client.map_mut(|c| c.stop());
        self.serial_client.map_mut(|s| s.stop());
        self.vnc_client.map_mut(|s| s.stop());
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_runner() {}
}
