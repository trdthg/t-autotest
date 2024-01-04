use std::time::Duration;

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

pub enum MsgRes {
    AssertScriptRun { res: String },
    AssertScreen { similarity: i32, ok: bool },
}
