use crate::{MsgReq, MsgRes, GLOBAL_BASE_SENDER};
use quick_js::JsValue;
use std::{
    sync::mpsc::{self, channel},
    time::Duration,
};
use tracing::{info, trace};

pub fn print(msg: String) -> JsValue {
    info!("api-print: [{msg}]");
    JsValue::Null
}

pub fn assert_script_run_ssh_seperate(cmd: String, timeout: i32) -> JsValue {
    trace!("assert_script_run::req");
    let msg_tx = unsafe { GLOBAL_BASE_SENDER.as_ref().unwrap().lock().unwrap().clone() };
    let (tx, rx) = channel::<MsgRes>();

    trace!("assert_script_run sending");
    msg_tx
        .send((
            MsgReq::AssertScriptRunSshSeperate {
                cmd,
                timeout: Duration::from_millis(timeout as u64),
            },
            tx,
        ))
        .unwrap();
    trace!("assert_script_run send done");

    trace!("assert_script_run waiting");
    let res = rx
        .recv_timeout(Duration::from_millis(timeout as u64))
        .unwrap();

    let res = if let MsgRes::AssertScriptRunSshSeperate { res } = res {
        JsValue::String(res)
    } else {
        JsValue::Null
    };
    trace!("assert_script_run done");
    res
}

pub fn assert_script_run_ssh_global(cmd: String, timeout: i32) -> JsValue {
    trace!("assert_script_run::req");
    let msg_tx = unsafe { GLOBAL_BASE_SENDER.as_ref().unwrap().lock().unwrap().clone() };
    let (tx, rx) = mpsc::channel::<MsgRes>();

    trace!("assert_script_run sending");
    msg_tx
        .send((
            MsgReq::AssertScriptRunSshGlobal {
                cmd,
                timeout: Duration::from_millis(timeout as u64),
            },
            tx,
        ))
        .unwrap();
    trace!("assert_script_run send done");

    trace!("assert_script_run waiting");

    let res = rx.recv().unwrap();
    let res = if let MsgRes::AssertScriptRunSshGlobal { res } = res {
        JsValue::String(res)
    } else {
        JsValue::Null
    };
    trace!("assert_script_run done");
    res
}

pub fn assert_script_run_serial_global(cmd: String, timeout: i32) -> JsValue {
    trace!("assert_script_run::req");
    let msg_tx = unsafe { GLOBAL_BASE_SENDER.as_ref().unwrap().lock().unwrap().clone() };
    let (tx, rx) = mpsc::channel::<MsgRes>();

    trace!("assert_script_run sending");
    msg_tx
        .send((
            MsgReq::AssertScriptRunSerialGlobal {
                cmd,
                timeout: Duration::from_millis(timeout as u64),
            },
            tx,
        ))
        .unwrap();
    trace!("assert_script_run send done");

    trace!("assert_script_run waiting");

    let res = rx.recv().unwrap();
    let res = if let MsgRes::AssertScriptRunSerialGlobal { res } = res {
        JsValue::String(res)
    } else {
        JsValue::Null
    };
    trace!("assert_script_run done");
    res
}

pub fn assert_screen(tags: String, timeout: i32) {
    trace!("assert_script_run pre");
    let msg_tx = unsafe { GLOBAL_BASE_SENDER.as_ref().unwrap().lock().unwrap().clone() };
    let (tx, rx) = channel::<MsgRes>();

    trace!("assert_script_run sending");
    msg_tx
        .send((
            MsgReq::AssertScreen {
                tag: tags,
                threshold: 1,
                timeout: Duration::from_millis(timeout as u64),
            },
            tx,
        ))
        .unwrap();
    trace!("assert_script_run send done");

    trace!("assert_script_run waiting");
    let res = rx
        .recv_timeout(Duration::from_millis(timeout as u64))
        .unwrap();

    let res = if let MsgRes::AssertScreen { similarity, ok } = res {
        JsValue::Int(similarity)
    } else {
        JsValue::Null
    };
    trace!("assert_script_run done");
}

// pub trait Callback<Args> {
//     fn call(&self, runner: &mut Runner, args: Args);
// }

// impl<F, P1> Callback<(P1,)> for F
// where
//     P1: Clone + 'static + Send + Sync,
//     F: Fn(&mut Runner, P1),
// {
//     fn call(&self, runner: &mut Runner, args: (P1,)) {
//         self(runner, args.0)
//     }
// }

// impl<F, P1, P2> Callback<(P1, P2)> for F
// where
//     P1: Clone + 'static + Send + Sync,
//     P2: Clone + 'static + Send + Sync,
//     F: Fn(&mut Runner, P1, P2),
// {
//     fn call(&self, runner: &mut Runner, args: (P1, P2)) {
//         self(runner, args.0, args.1)
//     }
// }

// macro_rules! impl_handler {
//     ($( $P:ident ),*) => {
//         impl<F, $($P,)*> SyncHandler<($($P,)*)> for F
//         where
//             $( $P: Clone + 'static + Send + Sync, )*
//             F: Fn($($P,)*),
//         {
//             fn call(&self) {
//                 self($(_e.get_data::<$P>(),)*);
//             }
//         }
//     };
// }

// impl_handler!();
// impl_handler!(P1);
// impl_handler!(P1, P2);
// impl_handler!(P1, P2, P3);
// impl_handler!(P1, P2, P3, P4);
// impl_handler!(P1, P2, P3, P4, P5);
// impl_handler!(P1, P2, P3, P4, P5, P6);
// impl_handler!(P1, P2, P3, P4, P5, P6, P7);
// impl_handler!(P1, P2, P3, P4, P5, P6, P7, P8);
// impl_handler!(P1, P2, P3, P4, P5, P6, P7, P8, P9);
