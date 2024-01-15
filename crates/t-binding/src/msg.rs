use std::time::Duration;

#[derive(Debug)]
pub enum MsgReq {
    // runner
    GetConfig {
        key: String,
    },
    // ssh
    SSHAssertScriptRunSeperate {
        cmd: String,
        timeout: Duration,
    },
    SSHAssertScriptRunGlobal {
        cmd: String,
        timeout: Duration,
    },
    SSHWriteStringGlobal {
        s: String,
    },
    // serial
    SerialAssertScriptRunGlobal {
        cmd: String,
        timeout: Duration,
    },
    SerialWriteStringGlobal {
        s: String,
    },
    // vnc
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
    Value(toml::Value),
    SSHAssertScriptRunSeperate(Result<String, MsgResError>),
    SSHAssertScriptRunGlobal(Result<String, MsgResError>),
    SerialAssertScriptRunGlobal(Result<String, MsgResError>),
    AssertScreen { similarity: i32, ok: bool },
}
