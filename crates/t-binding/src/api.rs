use crate::{
    get_global_sender,
    msg::{MsgResError, TextConsole},
    MsgReq, MsgRes,
};
use std::{sync::mpsc, time::Duration};
use tracing::{error, info, trace, Level};

pub fn print(level: tracing::Level, msg: String) {
    match level {
        Level::ERROR => info!(msg = "api print", level = ?level, msg = msg),
        Level::WARN => info!(msg = "api print", level = ?level, msg = msg),
        Level::INFO => info!(msg = "api print", level = ?level, msg = msg),
        Level::DEBUG => info!(msg = "api print", level = ?level, msg = msg),
        Level::TRACE => info!(msg = "api print", level = ?level, msg = msg),
    }
}

pub fn get_env(key: String) -> Option<String> {
    match req(MsgReq::GetConfig { key }) {
        MsgRes::ConfigValue(res) => res,
        _ => panic!("wrong msg type"),
    }
}

pub fn sleep(millis: u64) {
    std::thread::sleep(Duration::from_millis(millis));
}

pub fn req(req: MsgReq) -> MsgRes {
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
        Ok(res) => res,
        Err(e) => {
            error!(msg = "main runner loop tx closed", reason = ?e);
            panic!();
        }
    }
}

pub fn assert_script_run_global(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::ScriptRunGlobal {
        cmd,
        console: None,
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::ScriptRun(Ok(res)) => {
            if res.0 == 0 {
                Some(res.1)
            } else {
                None
            }
        }
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => None,
        _ => panic!("wrong msg type"),
    }
}

pub fn script_run_global(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::ScriptRunGlobal {
        cmd,
        console: None,
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::ScriptRun(Ok(res)) => Some(res.1),
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => None,
        _ => panic!("wrong msg type"),
    }
}

pub fn write_string(s: String) {
    match req(MsgReq::WriteStringGlobal { s, console: None }) {
        MsgRes::Done => {}
        _ => panic!("wrong msg type"),
    }
}

pub fn ssh_assert_script_run_seperate(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::SSHScriptRunSeperate {
        cmd,
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::ScriptRun(Ok(res)) => {
            if res.0 == 0 {
                Some(res.1)
            } else {
                None
            }
        }
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => None,
        _ => panic!("wrong msg type"),
    }
}

pub fn ssh_assert_script_run_global(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::ScriptRunGlobal {
        cmd,
        console: Some(TextConsole::SSH),
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::ScriptRun(Ok(res)) => {
            if res.0 == 0 {
                Some(res.1)
            } else {
                None
            }
        }
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => None,
        _ => panic!("wrong msg type"),
    }
}

pub fn ssh_script_run_global(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::ScriptRunGlobal {
        cmd,
        console: Some(TextConsole::SSH),
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::ScriptRun(Ok(res)) => Some(res.1),
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => None,
        _ => panic!("wrong msg type"),
    }
}

pub fn ssh_write_string(s: String) {
    match req(MsgReq::WriteStringGlobal {
        s,
        console: Some(TextConsole::SSH),
    }) {
        MsgRes::Done => {}
        _ => panic!("wrong msg type"),
    }
}

pub fn serial_script_run_global(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::ScriptRunGlobal {
        cmd,
        console: Some(TextConsole::Serial),
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::ScriptRun(Ok(res)) => Some(res.1),
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => None,
        _ => panic!("wrong msg type"),
    }
}

pub fn serial_assert_script_run_global(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::ScriptRunGlobal {
        cmd,
        console: Some(TextConsole::Serial),
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::ScriptRun(Ok(res)) => {
            if res.0 == 0 {
                Some(res.1)
            } else {
                None
            }
        }
        MsgRes::ScriptRun(Err(MsgResError::Timeout)) => None,
        _ => panic!("wrong msg type"),
    }
}

pub fn serial_write_string(s: String) {
    match req(MsgReq::WriteStringGlobal {
        s,
        console: Some(TextConsole::Serial),
    }) {
        MsgRes::Done => {}
        _ => panic!("wrong msg type"),
    }
}

pub fn vnc_check_screen(tag: String, timeout: i32) -> bool {
    let res = match req(MsgReq::AssertScreen {
        tag: tag.clone(),
        threshold: 1,
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::AssertScreen { similarity: 0, ok } => ok,
        _ => panic!("wrong msg type"), // timeout
    };
    info!(
        msg = "vnc_check_screen",
        api = "assert_screen",
        result = res,
        tag = tag
    );
    res
}

pub fn vnc_mouse_move(x: u16, y: u16) {
    assert!(matches!(req(MsgReq::MouseMove { x, y }), MsgRes::Done));
}

pub fn vnc_mouse_hide() {
    assert!(matches!(req(MsgReq::MouseHide), MsgRes::Done));
}

pub fn vnc_mouse_click() {
    assert!(matches!(req(MsgReq::MouseClick), MsgRes::Done));
}
