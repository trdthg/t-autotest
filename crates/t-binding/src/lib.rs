mod api;
mod engine;
mod msg;

use std::sync::{mpsc::Sender, RwLock};
use tracing::error;

pub use engine::{JSEngine, LuaEngine};
pub use msg::{MsgReq, MsgRes, MsgResError, TextConsole};

pub enum EngineError {}

pub trait ScriptEngine {
    fn run(&mut self, content: &str);
}

type GlobalSharedSender = Option<RwLock<Sender<(MsgReq, Sender<MsgRes>)>>>;

static mut GLOBAL_BASE_SENDER: GlobalSharedSender = None;

pub fn init(sender: Sender<(MsgReq, Sender<MsgRes>)>) {
    unsafe {
        GLOBAL_BASE_SENDER = Some(RwLock::new(sender));
    }
}

pub fn get_global_sender() -> Sender<(MsgReq, Sender<MsgRes>)> {
    if unsafe { GLOBAL_BASE_SENDER.is_none() } {
        error!(msg = "GLOBAL_BASE_SENDER is none, maybe init failed");
        panic!();
    }
    unsafe { GLOBAL_BASE_SENDER.as_ref().unwrap().read().unwrap().clone() }
}
