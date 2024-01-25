use std::time::Duration;

#[derive(Debug)]
pub enum TextConsole {
    SSH,
    Serial,
}

#[derive(Debug)]
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
    ScriptRunGlobal {
        console: Option<TextConsole>,
        cmd: String,
        timeout: Duration,
    },
    WriteStringGlobal {
        console: Option<TextConsole>,
        s: String,
    },
    WaitStringGlobal {
        console: Option<TextConsole>,
        s: String,
        timeout: Duration,
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
    ScriptRun(Result<(i32, String), MsgResError>),
    AssertScreen { similarity: i32, ok: bool },
}
