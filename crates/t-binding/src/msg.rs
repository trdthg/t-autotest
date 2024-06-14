use std::{sync::Arc, time::Duration};

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
    SetConfig {
        toml_str: String,
    },
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
        timeout: Duration,
    },
    VNC(VNC),
}

#[derive(Debug)]
pub enum VNC {
    TakeScreenShot,
    GetScreenShot,
    Refresh,
    CheckScreen {
        tag: String,
        threshold: f32,
        timeout: Duration,
        click: bool,
        r#move: bool,
        delay: Option<Duration>,
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
    Screenshot(Arc<PNG>),
}
