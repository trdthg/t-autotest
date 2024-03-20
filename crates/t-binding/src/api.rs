use super::error::{ApiError, Result};
use crate::{
    get_global_sender,
    msg::{MsgResError, TextConsole},
    MsgReq, MsgRes,
};
use std::{sync::mpsc, time::Duration};
use tracing::{error, info, trace, Level};

fn req(req: MsgReq) -> Result<MsgRes> {
    let msg_tx = get_global_sender();

    trace!(msg = "sending req");
    let (tx, rx) = mpsc::channel::<MsgRes>();
    if let Err(e) = msg_tx.send((req, tx)) {
        error!(msg = "main runner loop closed", reason = ?e);
    }

    trace!(msg = "waiting res");
    let res = rx.recv();

    trace!(msg = "received res");
    match res {
        Ok(res) => Ok(res),
        Err(_) => {
            error!(msg = "req failed, server already stopped");
            Err(ApiError::ServerStopped)
        }
    }
}

fn _script_run(cmd: String, console: Option<TextConsole>, timeout: i32) -> Result<(i32, String)> {
    match req(MsgReq::ScriptRunGlobal {
        cmd,
        console,
        timeout: Duration::from_millis(timeout as u64),
    })? {
        MsgRes::ScriptRun(Ok(res)) => Ok(res),
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => Err(ApiError::Timeout),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

fn _assert_script_run(cmd: String, console: Option<TextConsole>, timeout: i32) -> Result<String> {
    match req(MsgReq::ScriptRunGlobal {
        cmd,
        console,
        timeout: Duration::from_millis(timeout as u64),
    })? {
        MsgRes::ScriptRun(Ok(res)) => {
            if res.0 == 0 {
                Ok(res.1)
            } else {
                Err(ApiError::AssertFailed)
            }
        }
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => Err(ApiError::Timeout),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

fn _write_string(s: String, console: Option<TextConsole>) -> Result<()> {
    match req(MsgReq::WriteStringGlobal { s, console })? {
        MsgRes::Done => Ok(()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

fn _wait_string_ntimes(
    console: Option<TextConsole>,
    s: String,
    n: i32,
    timeout: i32,
) -> Result<()> {
    match req(MsgReq::WaitStringGlobal {
        console,
        s,
        n,
        timeout: Duration::from_secs(timeout as u64),
    })? {
        MsgRes::Done => Ok(()),
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

pub fn sleep(millis: u64) {
    std::thread::sleep(Duration::from_millis(millis));
}

pub fn get_env(key: String) -> Result<Option<String>> {
    match req(MsgReq::GetConfig { key })? {
        MsgRes::ConfigValue(res) => Ok(res),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

// default
pub fn script_run_global(cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(cmd, None, timeout)
}

pub fn assert_script_run_global(cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(cmd, None, timeout)
}

pub fn write_string(s: String) -> Result<()> {
    _write_string(s, None)
}

pub fn wait_string_ntimes(s: String, n: i32, timeout: i32) -> Result<()> {
    _wait_string_ntimes(None, s, n, timeout)
}

// serial
pub fn serial_script_run_global(cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(cmd, Some(TextConsole::Serial), timeout)
}

pub fn serial_assert_script_run_global(cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(cmd, Some(TextConsole::Serial), timeout)
}

pub fn serial_write_string(s: String) -> Result<()> {
    _write_string(s, Some(TextConsole::Serial))
}

// ssh
pub fn ssh_assert_script_run_seperate(cmd: String, timeout: i32) -> Result<String> {
    match req(MsgReq::SSHScriptRunSeperate {
        cmd,
        timeout: Duration::from_millis(timeout as u64),
    })? {
        MsgRes::ScriptRun(Ok(res)) => {
            if res.0 == 0 {
                Ok(res.1)
            } else {
                Err(ApiError::AssertFailed)
            }
        }
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => Err(ApiError::Timeout),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn ssh_script_run_global(cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(cmd, Some(TextConsole::SSH), timeout)
}

pub fn ssh_assert_script_run_global(cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(cmd, Some(TextConsole::SSH), timeout)
}

pub fn ssh_write_string(s: String) -> Result<()> {
    _write_string(s, Some(TextConsole::SSH))
}

// vnc
pub fn vnc_check_screen(tag: String, timeout: i32) -> Result<bool> {
    match req(MsgReq::AssertScreen {
        tag: tag.clone(),
        threshold: 1,
        timeout: Duration::from_millis(timeout as u64),
    })? {
        MsgRes::AssertScreen { similarity: 0, ok } => Ok(ok),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_assert_screen(tag: String, timeout: i32) -> Result<()> {
    let res = vnc_check_screen(tag, timeout)?;
    if res {
        Ok(())
    } else {
        Err(ApiError::AssertFailed)
    }
}

pub fn vnc_take_screenshot() -> Result<t_console::PNG> {
    if let MsgRes::Screenshot(res) = req(MsgReq::TakeScreenShot)? {
        return Ok(res);
    }
    Err(ApiError::ServerInvalidResponse)
}

pub fn vnc_mouse_move(x: u16, y: u16) -> Result<()> {
    if matches!(req(MsgReq::MouseMove { x, y })?, MsgRes::Done) {
        return Ok(());
    }
    Err(ApiError::ServerInvalidResponse)
}

pub fn vnc_mouse_keydown() -> Result<()> {
    if matches!(req(MsgReq::MouseKeyDown(true))?, MsgRes::Done) {
        return Ok(());
    }
    Err(ApiError::ServerInvalidResponse)
}

pub fn vnc_mouse_keyup() -> Result<()> {
    if matches!(req(MsgReq::MouseKeyDown(false))?, MsgRes::Done) {
        return Ok(());
    }
    Err(ApiError::ServerInvalidResponse)
}

pub fn vnc_mouse_hide() -> Result<()> {
    if matches!(req(MsgReq::MouseHide)?, MsgRes::Done) {
        return Ok(());
    }
    Err(ApiError::ServerInvalidResponse)
}

pub fn vnc_mouse_click() -> Result<()> {
    if matches!(req(MsgReq::MouseClick)?, MsgRes::Done) {
        return Ok(());
    }
    Err(ApiError::ServerInvalidResponse)
}

pub fn vnc_mouse_rclick() -> Result<()> {
    if matches!(req(MsgReq::MouseRClick)?, MsgRes::Done) {
        return Ok(());
    }
    Err(ApiError::ServerInvalidResponse)
}

pub fn vnc_send_key(s: String) -> Result<()> {
    if matches!(req(MsgReq::SendKey(s))?, MsgRes::Done) {
        return Ok(());
    }
    Err(ApiError::ServerInvalidResponse)
}

pub fn vnc_type_string(s: String) -> Result<()> {
    if matches!(req(MsgReq::TypeString(s))?, MsgRes::Done) {
        return Ok(());
    }
    Err(ApiError::ServerInvalidResponse)
}
