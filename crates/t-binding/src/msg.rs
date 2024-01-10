use std::time::Duration;

#[derive(Debug)]
pub enum MsgReq {
    AssertScriptRunSshSeperate {
        cmd: String,
        timeout: Duration,
    },
    AssertScriptRunSshGlobal {
        cmd: String,
        timeout: Duration,
    },
    AssertScriptRunSerialGlobal {
        cmd: String,
        timeout: Duration,
    },
    AssertScreen {
        tag: String,
        threshold: i32,
        timeout: Duration,
    },
    MouseMove {
        x: u16,
        y: u16,
    },
    MouseClick,
    MouseHide,
}

#[derive(Debug)]
pub enum MsgRes {
    Done,
    AssertScriptRunSshSeperate { res: String },
    AssertScriptRunSshGlobal { res: String },
    AssertScriptRunSerialGlobal { res: String },
    AssertScreen { similarity: i32, ok: bool },
}
