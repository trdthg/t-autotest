use super::error::{ApiError, Result};
use crate::{
    msg::{TextConsole, VNC},
    MsgReq, MsgRes, MsgResError,
};
use std::{sync::mpsc, time::Duration};
use tracing::{info, trace, Level};

pub type ApiTx = mpsc::Sender<(MsgReq, mpsc::Sender<MsgRes>)>;

#[derive(Clone)]
pub struct RustApi {
    pub tx: ApiTx,
}

impl RustApi {
    pub fn new(tx: ApiTx) -> Self {
        Self { tx }
    }
}

impl Api for RustApi {
    fn tx(&self) -> &ApiTx {
        &self.tx
    }
}

pub trait Api {
    fn tx(&self) -> &ApiTx;

    fn req(&self, req: MsgReq) -> Result<MsgRes> {
        let msg_tx = &self.tx();

        trace!(msg = "sending req");
        let (tx, rx) = mpsc::channel::<MsgRes>();
        msg_tx
            .send((req, tx))
            .map_err(|_| ApiError::ServerStopped)?;

        trace!(msg = "waiting res");
        let res = rx.recv().map_err(|_| ApiError::ServerStopped)?;
        trace!(msg = "received res");
        Ok(res)
    }

    fn _script_run(
        &self,
        cmd: String,
        console: Option<TextConsole>,
        timeout: i32,
    ) -> Result<(i32, String)> {
        match self.req(MsgReq::ScriptRun {
            cmd,
            console,
            timeout: Duration::from_secs(timeout as u64),
        })? {
            MsgRes::ScriptRun { code, value } => Ok((code, value)),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn _assert_script_run(
        &self,
        cmd: String,
        console: Option<TextConsole>,
        timeout: i32,
    ) -> Result<String> {
        match self.req(MsgReq::ScriptRun {
            cmd,
            console,
            timeout: Duration::from_secs(timeout as u64),
        })? {
            MsgRes::ScriptRun { code, value } => {
                if code == 0 {
                    Ok(value)
                } else {
                    Err(ApiError::AssertFailed)
                }
            }
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn _write(&self, s: String, console: Option<TextConsole>) -> Result<()> {
        match self.req(MsgReq::WriteString {
            s,
            console,
            timeout: Duration::from_secs(60),
        })? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn _wait_string_ntimes(
        &self,
        console: Option<TextConsole>,
        s: String,
        n: i32,
        timeout: i32,
    ) -> Result<bool> {
        match self.req(MsgReq::WaitString {
            console,
            s,
            n,
            timeout: Duration::from_secs(timeout as u64),
        })? {
            MsgRes::Done => Ok(true),
            MsgRes::Error(MsgResError::Timeout) => Ok(false),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    // general
    fn print(&self, level: tracing::Level, msg: String) {
        match level {
            Level::ERROR => info!(msg = "api print", level = ?level, msg = msg),
            Level::WARN => info!(msg = "api print", level = ?level, msg = msg),
            Level::INFO => info!(msg = "api print", level = ?level, msg = msg),
            Level::DEBUG => info!(msg = "api print", level = ?level, msg = msg),
            Level::TRACE => info!(msg = "api print", level = ?level, msg = msg),
        }
    }

    fn sleep(&self, secs: u64) {
        std::thread::sleep(Duration::from_secs(secs));
    }

    fn set_config(&self, toml_str: String) -> Result<Option<String>> {
        match self.req(MsgReq::SetConfig { toml_str })? {
            MsgRes::Done => Ok(None),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn get_env(&self, key: String) -> Result<Option<String>> {
        match self.req(MsgReq::GetConfig { key })? {
            MsgRes::ConfigValue(res) => Ok(res),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    // default
    fn script_run(&self, cmd: String, timeout: i32) -> Result<(i32, String)> {
        self._script_run(cmd, None, timeout)
    }

    fn assert_script_run(&self, cmd: String, timeout: i32) -> Result<String> {
        self._assert_script_run(cmd, None, timeout)
    }

    fn write(&self, s: String) -> Result<()> {
        self._write(s, None)
    }

    fn wait_string_ntimes(&self, s: String, n: i32, timeout: i32) -> Result<bool> {
        self._wait_string_ntimes(None, s, n, timeout)
    }

    // serial
    fn serial_script_run(&self, cmd: String, timeout: i32) -> Result<(i32, String)> {
        self._script_run(cmd, Some(TextConsole::Serial), timeout)
    }

    fn serial_assert_script_run(&self, cmd: String, timeout: i32) -> Result<String> {
        self._assert_script_run(cmd, Some(TextConsole::Serial), timeout)
    }

    fn serial_write(&self, s: String) -> Result<()> {
        self._write(s, Some(TextConsole::Serial))
    }

    // ssh
    fn ssh_assert_script_run_seperate(&self, cmd: String, timeout: i32) -> Result<String> {
        match self.req(MsgReq::SSHScriptRunSeperate {
            cmd,
            timeout: Duration::from_secs(timeout as u64),
        })? {
            MsgRes::ScriptRun { code, value } => {
                if code == 0 {
                    Ok(value)
                } else {
                    Err(ApiError::AssertFailed)
                }
            }
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn ssh_script_run(&self, cmd: String, timeout: i32) -> Result<(i32, String)> {
        self._script_run(cmd, Some(TextConsole::SSH), timeout)
    }

    fn ssh_assert_script_run(&self, cmd: String, timeout: i32) -> Result<String> {
        self._assert_script_run(cmd, Some(TextConsole::SSH), timeout)
    }

    fn ssh_write(&self, s: String) -> Result<()> {
        self._write(s, Some(TextConsole::SSH))
    }

    // vnc
    fn vnc_check_screen(&self, tag: String, timeout: i32) -> Result<bool> {
        match self.req(MsgReq::VNC(VNC::CheckScreen {
            tag: tag.clone(),
            threshold: 1,
            timeout: Duration::from_secs(timeout as u64),
        }))? {
            MsgRes::AssertScreen { similarity: _, ok } => Ok(ok),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_assert_screen(&self, tag: String, timeout: i32) -> Result<()> {
        if self.vnc_check_screen(tag, timeout)? {
            Ok(())
        } else {
            Err(ApiError::AssertFailed)
        }
    }

    fn vnc_refresh(&self) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::Refresh))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_take_screenshot(&self) -> Result<t_console::PNG> {
        match self.req(MsgReq::VNC(VNC::TakeScreenShot))? {
            MsgRes::Screenshot(res) => Ok(res),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_mouse_move(&self, x: u16, y: u16) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::MouseMove { x, y }))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_mouse_drag(&self, x: u16, y: u16) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::MouseDrag { x, y }))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_mouse_keydown(&self) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::MouseKeyDown(true)))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_mouse_keyup(&self) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::MouseKeyDown(false)))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_mouse_hide(&self) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::MouseHide))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_mouse_click(&self) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::MouseClick))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_mouse_rclick(&self) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::MouseRClick))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_send_key(&self, s: String) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::SendKey(s)))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }

    fn vnc_type_string(&self, s: String) -> Result<()> {
        match self.req(MsgReq::VNC(VNC::TypeString(s)))? {
            MsgRes::Done => Ok(()),
            MsgRes::Error(e) => Err(e.into()),
            _ => Err(ApiError::ServerInvalidResponse),
        }
    }
}
