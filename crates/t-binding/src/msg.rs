use std::time::Duration;

#[derive(Debug)]
pub enum MsgReq {
    SSHAssertScriptRunSeperate {
        cmd: String,
        timeout: Duration,
    },
    SSHAssertScriptRunGlobal {
        cmd: String,
        timeout: Duration,
    },
    SerialAssertScriptRunGlobal {
        cmd: String,
        timeout: Duration,
    },
    SerialWriteStringGlobal {
        s: String,
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
pub enum MsgResError {
    Timeout,
}

#[derive(Debug)]
pub enum MsgRes {
    Done,
    SSHAssertScriptRunSeperate(Result<String, MsgResError>),
    SSHAssertScriptRunGlobal(Result<String, MsgResError>),
    SerialAssertScriptRunGlobal(Result<String, MsgResError>),
    AssertScreen { similarity: i32, ok: bool },
}
