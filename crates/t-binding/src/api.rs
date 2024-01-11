use crate::{get_global_sender, MsgReq, MsgRes};
use std::{sync::mpsc, time::Duration};
use tracing::{error, info, trace};

pub fn print(msg: String) {
    info!("api-print: [{msg}]");
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

pub fn ssh_assert_script_run_seperate(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::AssertScriptRunSshSeperate {
        cmd,
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::AssertScriptRunSshSeperate { res } => Some(res),
        _ => panic!("wrong msg type"),
    }
}

pub fn ssh_assert_script_run_global(cmd: String, timeout: i32) -> Option<String> {
    match req(MsgReq::AssertScriptRunSshGlobal {
        cmd,
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::AssertScriptRunSshGlobal { res } => Some(res),
        _ => panic!("wrong msg type"),
    }
}

pub fn serial_assert_script_run_global(cmd: String, timeout: i32) -> String {
    match req(MsgReq::AssertScriptRunSerialGlobal {
        cmd,
        timeout: Duration::from_millis(timeout as u64),
    }) {
        MsgRes::AssertScriptRunSerialGlobal { res } => res,
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
