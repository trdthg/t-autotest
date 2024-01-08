mod api;
mod engine;
mod msg;

use std::sync::{mpsc::Sender, Mutex};

pub use engine::{JSEngine, LuaEngine};
pub use msg::{MsgReq, MsgRes};

pub enum EngineError {}

pub trait ScriptEngine {
    fn run(&mut self, content: &str);
}

static mut GLOBAL_BASE_SENDER: Option<Mutex<Sender<(MsgReq, Sender<MsgRes>)>>> = None;

pub fn init(sender: Sender<(MsgReq, Sender<MsgRes>)>) {
    unsafe {
        GLOBAL_BASE_SENDER = Some(Mutex::new(sender));
    }
}

pub fn get_global_sender() -> Sender<(MsgReq, Sender<MsgRes>)> {
    unsafe { GLOBAL_BASE_SENDER.as_ref().unwrap().lock().unwrap().clone() }
}
