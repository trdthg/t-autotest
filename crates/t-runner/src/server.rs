use crate::needle::NeedleManager;
use parking_lot::Mutex;
use std::{
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{self, Duration, Instant},
};
use t_binding::{MsgReq, MsgRes, MsgResError};
use t_config::{Config, Console};
use t_console::{key, ConsoleError, Serial, VNCEventReq, VNCEventRes, PNG, SSH, VNC};
use tracing::{error, info, trace, warn};

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

    screenshot_tx: Option<Sender<PNG>>,
}

pub struct ServerBuilder {
    config: Config,
    tx: Option<Sender<PNG>>,
    tx2: Option<Sender<PNG>>,
}

impl ServerBuilder {
    pub fn new(config: Config) -> Self {
        ServerBuilder {
            tx: None,
            tx2: None,
            config,
        }
    }

    pub fn with_vnc_screenshot_subscriber(mut self, tx: Sender<PNG>) -> Self {
        self.tx = Some(tx);
        self
    }

    pub fn with_latest_vnc_screenshot_subscriber(mut self, tx: Sender<PNG>) -> Self {
        self.tx2 = Some(tx);
        self
    }

    pub fn build(self) -> Result<(Server, mpsc::Sender<()>), ConsoleError> {
        let Console {
            ssh: _ssh,
            serial: _serial,
            vnc: _vnc,
        } = self.config.console.clone();

        // init serial
        let serial_client = if _serial.enable {
            Some(Serial::new(_serial)?)
        } else {
            None
        };

        // init ssh
        let ssh_client = if _ssh.enable {
            Some(SSH::new(_ssh)?)
        } else {
            None
        };

        // init vnc
        let vnc_client = if _vnc.enable {
            let addr = format!("{}:{}", _vnc.host, _vnc.port)
                .parse()
                .map_err(|e| {
                    ConsoleError::ConnectionBroken(format!("vnc addr is not valid, {}", e))
                })?;

            let vnc_client = VNC::connect(addr, _vnc.password.clone(), self.tx2)
                .map_err(|e| ConsoleError::ConnectionBroken(e.to_string()))?;

            info!(msg = "init vnc done");
            Some(vnc_client)
        } else {
            None
        };

        // init api request channel
        let (tx, msg_rx) = mpsc::channel();
        t_binding::init(tx);

        // init server
        let (stop_tx, stop_rx) = mpsc::channel();
        Ok((
            Server {
                config: self.config,
                msg_rx,
                stop_rx,
                screenshot_tx: self.tx,

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
            screenshot_tx,
        } = self;

        let _ssh_tty = ssh_client.map_ref(|c| c.tty());
        let serial_tty = serial_client.map_ref(|c| c.tty());

        loop {
            // stop on receive done signal
            if self.stop_rx.try_recv().is_ok() {
                info!(msg = "runner handler thread stopped");
                self.stop();
                break;
            }

            // handle msg
            let res = self.msg_rx.recv_timeout(Duration::from_secs(10));
            if res.is_err() {
                continue;
            }

            let (req, tx) = res.unwrap();
            trace!(msg = "recv request", req = ?req);
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
                MsgReq::TakeScreenShot => {
                    let (tx, rx) = mpsc::channel();

                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::TakeScreenShot, tx)))
                        .expect("no vnc")
                        .unwrap();
                    match rx.recv() {
                        Ok(VNCEventRes::Screen(res)) => MsgRes::Screenshot(res),
                        _ => MsgRes::Error(MsgResError::Timeout),
                    }
                }
                MsgReq::Refresh => {
                    let (tx, rx) = mpsc::channel();

                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::Refresh, tx)))
                        .expect("no vnc")
                        .unwrap();
                    match rx.recv() {
                        Ok(VNCEventRes::Done) => MsgRes::Done,
                        _ => MsgRes::Error(MsgResError::Timeout),
                    }
                }
                MsgReq::AssertScreen {
                    tag,
                    threshold: _,
                    timeout,
                } => {
                    let nmg = NeedleManager::new(&config.needle_dir);
                    let res: Result<(f32, bool, Option<t_console::PNG>), MsgResError> = {
                        let deadline = time::Instant::now() + timeout;
                        let mut similarity: f32 = 0.;
                        'res: loop {
                            if Instant::now() > deadline {
                                break 'res Ok((similarity, false, None));
                            }

                            let (tx, rx) = mpsc::channel();
                            vnc_client
                                .map_ref(|c| c.event_tx.send((VNCEventReq::TakeScreenShot, tx)))
                                .expect("no vnc")
                                .unwrap();
                            match rx.recv() {
                                Ok(VNCEventRes::Screen(s)) => {
                                    let Some((res_similarity, res)) = nmg.cmp(&s, &tag, None)
                                    else {
                                        error!(msg = "Needle file not found", tag = tag);
                                        break 'res Ok((similarity, false, Some(s)));
                                    };
                                    similarity = res_similarity;

                                    if !res {
                                        warn!(
                                            msg = "match failed",
                                            tag = tag,
                                            similarity = similarity
                                        );
                                    } else {
                                        info!(
                                            msg = "match success",
                                            tag = tag,
                                            similarity = similarity
                                        );
                                        break 'res Ok((similarity, res, Some(s)));
                                    }
                                }
                                Ok(_) => {
                                    warn!(msg = "invalid msg type");
                                }
                                Err(_e) => break Err(MsgResError::Timeout),
                            }
                            thread::sleep(Duration::from_millis(200));
                        }
                    };
                    if let Ok((similarity, same, png)) = res {
                        if let (Some(tx), Some(png)) = (&screenshot_tx, png) {
                            if tx.send(png).is_err() {
                                // TODO: handle ch close
                            }
                        }
                        MsgRes::AssertScreen {
                            similarity,
                            ok: same,
                        }
                    } else {
                        MsgRes::AssertScreen {
                            similarity: 0.,
                            ok: false,
                        }
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
                MsgReq::MouseDrag { x, y } => {
                    let (tx, rx) = mpsc::channel();
                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::MouseDrag(x, y), tx)))
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
                MsgReq::MouseKeyDown(down) => {
                    let (tx, rx) = mpsc::channel();
                    vnc_client
                        .map_ref(|c| {
                            c.event_tx.send((
                                if down {
                                    VNCEventReq::MoveDown(1)
                                } else {
                                    VNCEventReq::MoveUp(1)
                                },
                                tx,
                            ))
                        })
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
                MsgReq::MouseClick | MsgReq::MouseRClick => {
                    let button = match req {
                        MsgReq::MouseClick => 1,
                        MsgReq::MouseRClick => 1 << 2,
                        _ => unreachable!(),
                    };
                    let (tx, rx) = mpsc::channel();
                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::MoveDown(button), tx)))
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));

                    let (tx, rx) = mpsc::channel();

                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::MoveUp(button), tx)))
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
                MsgReq::SendKey(s) => {
                    let parts = s.split('-');
                    let mut keys = Vec::new();
                    for part in parts {
                        if let Some(key) = key::from_str(part) {
                            keys.push(key);
                        }
                    }
                    let (tx, rx) = mpsc::channel();
                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::SendKey { keys }, tx)))
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
                MsgReq::TypeString(s) => {
                    let (tx, rx) = mpsc::channel();
                    vnc_client
                        .map_ref(|c| c.event_tx.send((VNCEventReq::TypeString(s), tx)))
                        .unwrap()
                        .unwrap();
                    assert!(matches!(rx.recv().unwrap(), VNCEventRes::Done));
                    MsgRes::Done
                }
            };

            // if handle req, take a screenshot
            Self::send_screenshot(vnc_client, screenshot_tx);

            trace!(msg = format!("sending res: {:?}", res));
            if let Err(e) = tx.send(res) {
                info!(msg = "script engine receiver closed", reason = ?e);
                break;
            }
        }
        info!(msg = "Runner loop stopped")
    }

    fn send_screenshot(vnc_client: &AMOption<VNC>, screenshot_tx: &Option<Sender<PNG>>) {
        let (tx, rx) = mpsc::channel();

        vnc_client
            .map_ref(|c| c.event_tx.send((VNCEventReq::TakeScreenShot, tx)))
            .expect("no vnc")
            .unwrap();
        if let Ok(VNCEventRes::Screen(png)) = rx.recv() {
            if let Some(tx) = screenshot_tx {
                if tx.send(png).is_err() {
                    // TODO: handle ch close
                }
            }
        }
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
