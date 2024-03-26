use pyo3::Python;
use std::thread;
use std::{sync::mpsc, time::Duration};
use t_binding::error::{ApiError, Result};
use t_binding::{
    get_global_sender,
    msg::{MsgResError, TextConsole},
    MsgReq, MsgRes,
};
use tracing::{info, trace, warn, Level};

fn req(py: Python<'_>, req: MsgReq) -> Result<MsgRes> {
    let msg_tx = get_global_sender();

    trace!(msg = "sending req");
    let (tx, rx) = mpsc::channel::<MsgRes>();
    msg_tx
        .send((req, tx))
        .map_err(|_| ApiError::ServerStopped)?;

    trace!(msg = "waiting res");
    loop {
        match rx.try_recv() {
            Ok(res) => {
                trace!(msg = "received res");
                return Ok(res);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => return Err(ApiError::ServerStopped),
        }
        py.check_signals().map_err(|_| ApiError::Interrupt)?;
        thread::sleep(Duration::from_millis(100));
    }
}

fn _script_run(
    py: Python<'_>,
    cmd: String,
    console: Option<TextConsole>,
    timeout: i32,
) -> Result<(i32, String)> {
    match req(
        py,
        MsgReq::ScriptRun {
            cmd,
            console,
            timeout: Duration::from_secs(timeout as u64),
        },
    )? {
        MsgRes::ScriptRun { code, value } => Ok((code, value)),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

fn _assert_script_run(
    py: Python<'_>,
    cmd: String,
    console: Option<TextConsole>,
    timeout: i32,
) -> Result<String> {
    match req(
        py,
        MsgReq::ScriptRun {
            cmd,
            console,
            timeout: Duration::from_secs(timeout as u64),
        },
    )? {
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

fn _write_string(py: Python<'_>, s: String, console: Option<TextConsole>) -> Result<()> {
    match req(
        py,
        MsgReq::WriteString {
            s,
            console,
            timeout: Duration::from_secs(60),
        },
    )? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

fn _wait_string_ntimes(
    py: Python<'_>,
    console: Option<TextConsole>,
    s: String,
    n: i32,
    timeout: i32,
) -> Result<bool> {
    match req(
        py,
        MsgReq::WaitString {
            console,
            s,
            n,
            timeout: Duration::from_secs(timeout as u64),
        },
    )? {
        MsgRes::Done => Ok(true),
        MsgRes::Error(MsgResError::Timeout) => Ok(false),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

// general
pub fn print(py: Python<'_>, level: tracing::Level, msg: String) {
    match level {
        Level::ERROR => info!(msg = "api print", level = ?level, msg = msg),
        Level::WARN => info!(msg = "api print", level = ?level, msg = msg),
        Level::INFO => info!(msg = "api print", level = ?level, msg = msg),
        Level::DEBUG => info!(msg = "api print", level = ?level, msg = msg),
        Level::TRACE => info!(msg = "api print", level = ?level, msg = msg),
    }
}

pub fn sleep(py: Python<'_>, secs: u64) {
    for i in 0..secs {
        std::thread::sleep(Duration::from_secs(1));
        py.check_signals();
    }
}

pub fn get_env(py: Python<'_>, key: String) -> Result<Option<String>> {
    match req(py, MsgReq::GetConfig { key })? {
        MsgRes::ConfigValue(res) => Ok(res),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

// default
pub fn script_run(py: Python<'_>, cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(py, cmd, None, timeout)
}

pub fn assert_script_run(py: Python<'_>, cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(py, cmd, None, timeout)
}

pub fn write(py: Python<'_>, s: String) -> Result<()> {
    _write_string(py, s, None)
}

pub fn wait_string_ntimes(py: Python<'_>, s: String, n: i32, timeout: i32) -> Result<bool> {
    _wait_string_ntimes(py, None, s, n, timeout)
}

// serial
pub fn serial_script_run(py: Python<'_>, cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(py, cmd, Some(TextConsole::Serial), timeout)
}

pub fn serial_assert_script_run(py: Python<'_>, cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(py, cmd, Some(TextConsole::Serial), timeout)
}

pub fn serial_write(py: Python<'_>, s: String) -> Result<()> {
    _write_string(py, s, Some(TextConsole::Serial))
}

// ssh
pub fn ssh_assert_script_run_seperate(py: Python<'_>, cmd: String, timeout: i32) -> Result<String> {
    match req(
        py,
        MsgReq::SSHScriptRunSeperate {
            cmd,
            timeout: Duration::from_secs(timeout as u64),
        },
    )? {
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

pub fn ssh_script_run(py: Python<'_>, cmd: String, timeout: i32) -> Result<(i32, String)> {
    _script_run(py, cmd, Some(TextConsole::SSH), timeout)
}

pub fn ssh_assert_script_run(py: Python<'_>, cmd: String, timeout: i32) -> Result<String> {
    _assert_script_run(py, cmd, Some(TextConsole::SSH), timeout)
}

pub fn ssh_write(py: Python<'_>, s: String) -> Result<()> {
    _write_string(py, s, Some(TextConsole::SSH))
}

// vnc
pub fn vnc_check_screen(py: Python<'_>, tag: String, timeout: i32) -> Result<bool> {
    match req(
        py,
        MsgReq::AssertScreen {
            tag: tag.clone(),
            threshold: 1,
            timeout: Duration::from_secs(timeout as u64),
        },
    )? {
        MsgRes::AssertScreen { similarity: _, ok } => Ok(ok),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_assert_screen(py: Python<'_>, tag: String, timeout: i32) -> Result<()> {
    if vnc_check_screen(py, tag, timeout)? {
        Ok(())
    } else {
        Err(ApiError::AssertFailed)
    }
}

pub fn vnc_refresh(py: Python<'_>) -> Result<()> {
    match req(py, MsgReq::Refresh)? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_take_screenshot(py: Python<'_>) -> Result<t_console::PNG> {
    match req(py, MsgReq::TakeScreenShot)? {
        MsgRes::Screenshot(res) => Ok(res),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_move(py: Python<'_>, x: u16, y: u16) -> Result<()> {
    match req(py, MsgReq::MouseMove { x, y })? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_drag(py: Python<'_>, x: u16, y: u16) -> Result<()> {
    match req(py, MsgReq::MouseDrag { x, y })? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_keydown(py: Python<'_>) -> Result<()> {
    match req(py, MsgReq::MouseKeyDown(true))? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_keyup(py: Python<'_>) -> Result<()> {
    match req(py, MsgReq::MouseKeyDown(false))? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_hide(py: Python<'_>) -> Result<()> {
    match req(py, MsgReq::MouseHide)? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_click(py: Python<'_>) -> Result<()> {
    match req(py, MsgReq::MouseClick)? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_mouse_rclick(py: Python<'_>) -> Result<()> {
    match req(py, MsgReq::MouseRClick)? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_send_key(py: Python<'_>, s: String) -> Result<()> {
    match req(py, MsgReq::SendKey(s))? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}

pub fn vnc_type_string(py: Python<'_>, s: String) -> Result<()> {
    match req(py, MsgReq::TypeString(s))? {
        MsgRes::Done => Ok(()),
        MsgRes::Error(e) => Err(e.into()),
        _ => Err(ApiError::ServerInvalidResponse),
    }
}
