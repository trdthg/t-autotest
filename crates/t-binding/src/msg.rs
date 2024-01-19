use std::time::Duration;

#[derive(Debug)]
#[non_exhaustive]
pub enum MsgReq {
    // runner
    GetConfig {
        key: String,
    },
    // ssh
    SSHScriptRunSeperate {
        cmd: String,
        timeout: Duration,
    },
    SSHScriptRunGlobal {
        cmd: String,
        timeout: Duration,
    },
    SSHWriteStringGlobal {
        s: String,
    },
    // serial
    SerialScriptRunGlobal {
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
#[non_exhaustive]
pub enum MsgResError {
    Timeout,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum MsgRes {
    Done,
    Value(toml::Value),
    SSHScriptRunSeperate(Result<String, MsgResError>),
    SSHScriptRunGlobal(Result<String, MsgResError>),
    SerialScriptRunGlobal(Result<(i32, String), MsgResError>),
    AssertScreen { similarity: i32, ok: bool },
}
