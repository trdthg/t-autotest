use super::error::{ApiError, Result};
use crate::{get_global_sender, msg::TextConsole, MsgReq, MsgRes, MsgResError};
use std::{sync::mpsc, time::Duration};
use tracing::{info, trace, Level};

fn req(req: MsgReq) -> Result<MsgRes> {
    let msg_tx = get_global_sender();

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

fn _script_run(cmd: String, console: Option<TextConsole>, timeout: i32) -> Result<(i32, String)> {
    match req(MsgReq::ScriptRun {
        cmd,
        console,
        timeout: Duration::from_secs(timeout as u64),
    })? {
        MsgRes::ScriptRun { code, value } => Ok((code, value)),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

fn _assert_script_run(cmd: String, console: Option<TextConsole>, timeout: i32) -> Result<String> {
    match req(MsgReq::ScriptRun {
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

fn _write(s: String, console: Option<TextConsole>) -> Result<()> {
    match req(MsgReq::WriteString {
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
    console: Option<TextConsole>,
    s: String,
    n: i32,
    timeout: i32,
) -> Result<bool> {
    match req(MsgReq::WaitString {
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
pub fn print(level: tracing::Level, msg: String) {
    match level {
        Level::ERROR => info!(msg = "api print", level = ?level, msg = msg),
        Level::WARN => info!(msg = "api print", level = ?level, msg = msg),
        Level::INFO => info!(msg = "api print", level = ?level, msg = msg),
        Level::DEBUG => info!(msg = "api print", level = ?level, msg = msg),
        Level::TRACE => info!(msg = "api print", level = ?level, msg = msg),
    }
}

pub fn sleep(secs: u64) {
    std::thread::sleep(Duration::from_secs(secs));
}

pub fn set_config(toml_str: String) -> Result<Option<String>> {
    match req(MsgReq::SetConfig { toml_str })? {
        MsgRes::Done => Ok(None),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn get_env(key: String) -> Result<Option<String>> {
    match req(MsgReq::GetConfig { key })? {
        MsgRes::ConfigValue(res) => Ok(res),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

// default
pub fn script_run(cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(cmd, None, timeout)
}

pub fn assert_script_run(cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(cmd, None, timeout)
}

pub fn write(s: String) -> Result<()> {
    _write(s, None)
}

pub fn wait_string_ntimes(s: String, n: i32, timeout: i32) -> Result<bool> {
    _wait_string_ntimes(None, s, n, timeout)
}

// serial
pub fn serial_script_run(cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(cmd, Some(TextConsole::Serial), timeout)
}

pub fn serial_assert_script_run(cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(cmd, Some(TextConsole::Serial), timeout)
}

pub fn serial_write(s: String) -> Result<()> {
    _write(s, Some(TextConsole::Serial))
}

// ssh
pub fn ssh_assert_script_run_seperate(cmd: String, timeout: i32) -> Result<String> {
    match req(MsgReq::SSHScriptRunSeperate {
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

pub fn ssh_script_run(cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(cmd, Some(TextConsole::SSH), timeout)
}

pub fn ssh_assert_script_run(cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(cmd, Some(TextConsole::SSH), timeout)
}

pub fn ssh_write(s: String) -> Result<()> {
    _write(s, Some(TextConsole::SSH))
}

// vnc
pub fn vnc_check_screen(tag: String, timeout: i32) -> Result<bool> {
    match req(MsgReq::AssertScreen {
        tag: tag.clone(),
        threshold: 1,
        timeout: Duration::from_secs(timeout as u64),
    })? {
        MsgRes::AssertScreen { similarity: _, ok } => Ok(ok),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_assert_screen(tag: String, timeout: i32) -> Result<()> {
    if vnc_check_screen(tag, timeout)? {
        Ok(())
    } else {
        Err(ApiError::AssertFailed)
    }
}

pub fn vnc_refresh() -> Result<()> {
    match req(MsgReq::Refresh)? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_take_screenshot() -> Result<t_console::PNG> {
    match req(MsgReq::TakeScreenShot)? {
        MsgRes::Screenshot(res) => Ok(res),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_move(x: u16, y: u16) -> Result<()> {
    match req(MsgReq::MouseMove { x, y })? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_drag(x: u16, y: u16) -> Result<()> {
    match req(MsgReq::MouseDrag { x, y })? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_keydown() -> Result<()> {
    match req(MsgReq::MouseKeyDown(true))? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_keyup() -> Result<()> {
    match req(MsgReq::MouseKeyDown(false))? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_hide() -> Result<()> {
    match req(MsgReq::MouseHide)? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_click() -> Result<()> {
    match req(MsgReq::MouseClick)? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_rclick() -> Result<()> {
    match req(MsgReq::MouseRClick)? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_send_key(s: String) -> Result<()> {
    match req(MsgReq::SendKey(s))? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_type_string(s: String) -> Result<()> {
    match req(MsgReq::TypeString(s))? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}
