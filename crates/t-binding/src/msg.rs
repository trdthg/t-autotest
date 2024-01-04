use std::time::Duration;

#[derive(Debug)]
pub enum MsgReq {
    AssertScriptRun {
        cmd: String,
        timeout: Duration,
    },
    AssertScreen {
        tag: String,
        threshold: i32,
        timeout: Duration,
    },
}

#[derive(Debug)]
pub enum MsgRes {
    AssertScriptRun { res: String },
    AssertScreen { similarity: i32, ok: bool },
}
