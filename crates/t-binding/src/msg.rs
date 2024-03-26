use std::time::Duration;

use t_console::PNG;

use crate::ApiError;

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
    ScriptRun {
        console: Option<TextConsole>,
        cmd: String,
        timeout: Duration,
    },
    WriteString {
        console: Option<TextConsole>,
        s: String,
        timeout: Duration,
    },
    WaitString {
        console: Option<TextConsole>,
        s: String,
        n: i32,
        timeout: Duration,
    },
    // vnc
    TakeScreenShot,
    Refresh,
    AssertScreen {
        tag: String,
        threshold: i32,
        timeout: Duration,
    },
    MouseMove {
        x: u16,
        y: u16,
    },
    MouseDrag {
        x: u16,
        y: u16,
    },
    MouseHide,
    MouseClick,
    MouseRClick,
    MouseKeyDown(bool),
    SendKey(String),
    TypeString(String),
}

#[derive(Debug)]
pub enum MsgResError {
    Timeout,
    String(String),
}

impl From<MsgResError> for ApiError {
    fn from(value: MsgResError) -> Self {
        match value {
            MsgResError::Timeout => Self::Timeout,
            MsgResError::String(s) => Self::String(s),
        }
    }
}

#[derive(Debug)]
pub enum MsgRes {
    Done,
    ConfigValue(Option<String>),
    ScriptRun { code: i32, value: String },
    Error(MsgResError),
    AssertScreen { similarity: f32, ok: bool },
    Screenshot(PNG),
}
