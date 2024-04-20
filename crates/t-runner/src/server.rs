use crate::needle::NeedleManager;
use parking_lot::Mutex;
use std::{
    env::current_dir,
    path::PathBuf,
    str::FromStr,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
    time::{self, Duration, Instant},
};
use t_binding::{api::ApiTx, MsgReq, MsgRes, MsgResError};
use t_config::{Config, ConsoleVNC};
use t_console::{key, ConsoleError, Serial, VNCEventReq, VNCEventRes, PNG, SSH, VNC};
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

    fn set(&mut self, val: Option<T>) {
        self.inner = Arc::new(Mutex::new(val));
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
    config: Option<Config>,

    msg_rx: Receiver<(MsgReq, Sender<MsgRes>)>,

    stop_rx: mpsc::Receiver<()>,

    ssh: AMOption<SSH>,
    serial: AMOption<Serial>,
    vnc: AMOption<VNC>,

    screenshot_tx: Option<Sender<PNG>>,
}

pub struct ServerBuilder {
    config: Option<Config>,
    tx: Option<Sender<PNG>>,
}

impl ServerBuilder {
    pub fn new(config: Option<Config>) -> Self {
        ServerBuilder { tx: None, config }
    }

    pub fn with_vnc_screenshot_subscriber(mut self, tx: Sender<PNG>) -> Self {
        self.tx = Some(tx);
        self
    }

    pub fn build(self) -> Result<(Server, ApiTx, mpsc::Sender<()>), ConsoleError> {
        let c = self.config;

        // init api request channel
        let (tx, msg_rx) = mpsc::channel();

        // init server
        let (stop_tx, stop_rx) = mpsc::channel();
        let mut server = Server {
            config: c.clone(),
            msg_rx,
            stop_rx,
            screenshot_tx: self.tx,

            ssh: AMOption::new(None),
            serial: AMOption::new(None),
            vnc: AMOption::new(None),
        };

        if let Some(c) = c {
            server.connect_with_config(&c)?;
        }

        Ok((server, tx, stop_tx))
    }
}

impl Server {
    pub fn start_non_blocking(self) {
        thread::spawn(move || {
            let mut s = self;
            s.pool();
        });
    }

    fn connect_with_config<'a>(&'a mut self, c: &'a Config) -> Result<(), ConsoleError> {
        // init serial
        if let Some(c) = c.serial.clone() {
            self.serial.map_ref(|c| c.stop());
            match Serial::new(c) {
                Ok(s) => {
                    self.serial.set(Some(s));
                    info!("serial connect success");
                }
                Err(e) => {
                    error!(msg="serial connect failed", reason = ?e);
                    return Err(e);
                }
            }
        }

        // init ssh
        if let Some(c) = c.ssh.clone() {
            self.ssh.map_ref(|s| s.stop());
            match SSH::new(c) {
                Ok(s) => {
                    self.ssh.set(Some(s));
                    info!("ssh connect success");
                }
                Err(e) => {
                    error!(msg="ssh connect failed", reason = ?e);
                    return Err(e);
                }
            }
        }

        // init vnc
        let build_vnc = move |vnc: ConsoleVNC| {
            let addr = format!("{}:{}", vnc.host, vnc.port)
                .parse()
                .map_err(|e| ConsoleError::NoConnection(format!("vnc addr is not valid, {}", e)))?;

            let vnc_client = VNC::connect(addr, vnc.password.clone(), None)
                .map_err(|e| ConsoleError::NoConnection(e.to_string()))?;
            Ok::<VNC, ConsoleError>(vnc_client)
        };
        match c.vnc.clone().map(build_vnc) {
            Some(Ok(s)) => {
                self.vnc.set(Some(s));
                info!("vnc connect success");
            }
            Some(Err(e)) => {
                error!(msg="ssh connect failed", reason = ?e);
                return Err(e);
            }
            None => {}
        }
        Ok(())
    }

    fn pool(&mut self) {
        // start script engine if in case mode
        info!(msg = "start msg handler thread");

        let nmg = NeedleManager::new(
            self.config
                .as_ref()
                .and_then(|c| c.vnc.as_ref().and_then(|vnc| vnc.needle_dir.as_ref()))
                .map_or(current_dir().ok(), |c| PathBuf::from_str(c).ok())
                .unwrap(),
        );

        loop {
            // stop on receive done signal
            if self.stop_rx.try_recv().is_ok() {
                info!(msg = "runner handler thread stopped");
                self.stop();
                break;
            }

            // handle msg
            match self.msg_rx.try_recv() {
                Ok((req, tx)) => {
                    let mut enable_log = true;
                    if matches!(req, MsgReq::VNC(t_binding::msg::VNC::TakeScreenShot)) {
                        enable_log = false;
                    }

                    if enable_log {
                        info!("server recv req: {:?}", req);
                    }

                    let res = match req {
                        // common
                        MsgReq::SetConfig { toml_str } => match Config::from_toml_str(&toml_str) {
                            Ok(c) => match &mut self.connect_with_config(&c) {
                                Ok(()) => {
                                    self.config = Some(c);
                                    MsgRes::Done
                                }
                                Err(e) => MsgRes::Error(MsgResError::String(format!(
                                    "connect failed, reason = {}",
                                    e
                                ))),
                            },
                            Err(e) => MsgRes::Error(MsgResError::String(format!(
                                "config invalid, reason = {}",
                                e
                            ))),
                        },
                        MsgReq::GetConfig { key } => {
                            let v = self
                                .config
                                .as_ref()
                                .and_then(|c| c.env.as_ref())
                                .and_then(|e| e.get(&key).map(|v| v.to_string()));
                            MsgRes::ConfigValue(v)
                        }
                        // ssh
                        MsgReq::SSHScriptRunSeperate { cmd, timeout: _ } => {
                            let client = &self.ssh;
                            let res = client
                                .map_mut(|c| c.exec_seperate(&cmd))
                                .unwrap_or(Ok((-1, "no ssh".to_string())))
                                .map_err(|_| MsgResError::Timeout);
                            match res {
                                Ok((code, value)) => MsgRes::ScriptRun { code, value },
                                Err(e) => MsgRes::Error(e),
                            }
                        }
                        MsgReq::ScriptRun {
                            cmd,
                            console,
                            timeout,
                        } => {
                            let res = match (console, self.ssh.is_some(), self.serial.is_some()) {
                                (None | Some(t_binding::TextConsole::Serial), _, true) => self
                                    .serial
                                    .map_mut(|c| c.exec(timeout, &cmd))
                                    .unwrap_or(Ok((1, "no serial".to_string())))
                                    .map_err(|_| MsgResError::Timeout),
                                (None | Some(t_binding::TextConsole::SSH), true, _) => self
                                    .ssh
                                    .map_mut(|c| c.exec(timeout, &cmd))
                                    .unwrap_or(Ok((-1, "no ssh".to_string())))
                                    .map_err(|_| MsgResError::Timeout),
                                _ => Err(MsgResError::String("no console supported".to_string())),
                            };
                            match res {
                                Ok((code, value)) => MsgRes::ScriptRun { code, value },
                                Err(e) => MsgRes::Error(e),
                            }
                        }
                        MsgReq::WriteString {
                            console,
                            s,
                            timeout,
                        } => {
                            if let Err(e) =
                                match (console, self.ssh.is_some(), self.serial.is_some()) {
                                    (None | Some(t_binding::TextConsole::Serial), _, true) => self
                                        .serial
                                        .map_mut(|c| c.write_string(&s, timeout))
                                        .expect("no serial")
                                        .map_err(|_| MsgResError::Timeout),
                                    (None | Some(t_binding::TextConsole::SSH), true, _) => self
                                        .ssh
                                        .map_mut(|c| c.write_string(&s, timeout))
                                        .expect("no ssh")
                                        .map_err(|_| MsgResError::Timeout),
                                    _ => {
                                        Err(MsgResError::String("no console supported".to_string()))
                                    }
                                }
                            {
                                MsgRes::Error(e)
                            } else {
                                MsgRes::Done
                            }
                        }
                        MsgReq::WaitString {
                            console,
                            s,
                            n,
                            timeout,
                        } => {
                            if let Err(e) =
                                match (console, self.ssh.is_some(), self.serial.is_some()) {
                                    (None | Some(t_binding::TextConsole::Serial), _, true) => self
                                        .serial
                                        .map_mut(|c| c.wait_string_ntimes(timeout, &s, n as usize))
                                        .expect("no serial")
                                        .map_err(|_| MsgResError::Timeout),
                                    (None | Some(t_binding::TextConsole::SSH), true, _) => self
                                        .ssh
                                        .map_mut(|c| c.wait_string_ntimes(timeout, &s, n as usize))
                                        .expect("no ssh")
                                        .map_err(|_| MsgResError::Timeout),
                                    _ => {
                                        Err(MsgResError::String("no console supported".to_string()))
                                    }
                                }
                            {
                                MsgRes::Error(e)
                            } else {
                                MsgRes::Done
                            }
                        }
                        MsgReq::VNC(e) => {
                            if let Some(res) = self.vnc.map_ref(|c| {
                                let res = match e {
                                    t_binding::msg::VNC::TakeScreenShot => {
                                        match c.send(VNCEventReq::TakeScreenShot) {
                                            Ok(VNCEventRes::Screen(res)) => MsgRes::Screenshot(res),
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                    t_binding::msg::VNC::Refresh => {
                                        match c.send(VNCEventReq::Refresh) {
                                            Ok(VNCEventRes::Screen(res)) => MsgRes::Screenshot(res),
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                    t_binding::msg::VNC::CheckScreen {
                                        tag,
                                        threshold,
                                        timeout,
                                    } => {
                                        let res: Result<
                                            (f32, bool, Option<t_console::PNG>),
                                            MsgResError,
                                        > = {
                                            let deadline = time::Instant::now() + timeout;
                                            let mut similarity: f32 = 0.;
                                            'res: loop {
                                                if Instant::now() > deadline {
                                                    break 'res Ok((similarity, false, None));
                                                }
                                                match c.send(VNCEventReq::TakeScreenShot) {
                                                    Ok(VNCEventRes::Screen(s)) => {
                                                        let Some((res_similarity, res)) = nmg.cmp(
                                                            &s,
                                                            &tag,
                                                            Some(threshold as f32),
                                                        ) else {
                                                            error!(
                                                                msg = "Needle file not found",
                                                                tag = tag
                                                            );
                                                            break 'res Ok((
                                                                similarity,
                                                                false,
                                                                Some(s),
                                                            ));
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
                                                            break 'res Ok((
                                                                similarity,
                                                                res,
                                                                Some(s),
                                                            ));
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
                                            if let (Some(tx), Some(png)) =
                                                (&self.screenshot_tx, png)
                                            {
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
                                    t_binding::msg::VNC::MouseMove { x, y } => {
                                        match c.send(VNCEventReq::MouseMove(x, y)) {
                                            Ok(VNCEventRes::Done) => MsgRes::Done,
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                    t_binding::msg::VNC::MouseDrag { x, y } => {
                                        match c.send(VNCEventReq::MouseDrag(x, y)) {
                                            Ok(VNCEventRes::Done) => MsgRes::Done,
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                    t_binding::msg::VNC::MouseHide => {
                                        match c.send(VNCEventReq::MouseHide) {
                                            Ok(VNCEventRes::Done) => MsgRes::Done,
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                    t_binding::msg::VNC::MouseClick
                                    | t_binding::msg::VNC::MouseRClick => {
                                        let button = match e {
                                            t_binding::msg::VNC::MouseClick => 1,
                                            t_binding::msg::VNC::MouseRClick => 1 << 2,
                                            _ => unreachable!(),
                                        };
                                        match c.send(VNCEventReq::MoveDown(button)) {
                                            Ok(VNCEventRes::Done) => {
                                                match c.send(VNCEventReq::MoveUp(button)) {
                                                    Ok(VNCEventRes::Done) => MsgRes::Done,
                                                    _ => MsgRes::Error(MsgResError::Timeout),
                                                }
                                            }
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                    t_binding::msg::VNC::MouseKeyDown(down) => {
                                        match c.send(if down {
                                            VNCEventReq::MoveDown(1)
                                        } else {
                                            VNCEventReq::MoveUp(1)
                                        }) {
                                            Ok(VNCEventRes::Done) => MsgRes::Done,
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                    t_binding::msg::VNC::SendKey(s) => {
                                        let parts = s.split('-');
                                        let mut keys = Vec::new();
                                        for part in parts {
                                            if let Some(key) = key::from_str(part) {
                                                keys.push(key);
                                            }
                                        }
                                        match c.send(VNCEventReq::SendKey { keys }) {
                                            Ok(VNCEventRes::Done) => MsgRes::Done,
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                    t_binding::msg::VNC::TypeString(s) => {
                                        match c.send(VNCEventReq::TypeString(s)) {
                                            Ok(VNCEventRes::Done) => MsgRes::Done,
                                            _ => MsgRes::Error(MsgResError::Timeout),
                                        }
                                    }
                                };
                                res
                            }) {
                                res
                            } else {
                                MsgRes::Error(MsgResError::String("no vnc".to_string()))
                            }
                        }
                    };

                    // if handle req, take a screenshot
                    Self::send_screenshot(&self.vnc, &self.screenshot_tx);

                    if enable_log {
                        info!(msg = format!("sending res: {:?}", res));
                    }

                    if let Err(e) = tx.send(res) {
                        warn!(msg = "script engine receiver closed", reason = ?e);
                    }
                }
                Err(e) => match e {
                    mpsc::TryRecvError::Empty => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    mpsc::TryRecvError::Disconnected => {
                        warn!(msg = "request sender closed unexpected", reason = ?e);
                        break;
                    }
                },
            }
        }
        info!(msg = "Runner loop stopped")
    }

    fn send_screenshot(vnc_client: &AMOption<VNC>, screenshot_tx: &Option<Sender<PNG>>) {
        if !vnc_client.is_some() {
            return;
        }

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
        self.ssh.map_mut(|c| c.stop());
        self.serial.map_mut(|s| s.stop());
        self.vnc.map_mut(|s| s.stop());
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_runner() {}
}
