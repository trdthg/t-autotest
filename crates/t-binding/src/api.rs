use crate::{get_global_sender, MsgReq, MsgRes, GLOBAL_BASE_SENDER};
use quick_js::JsValue;
use std::{
    sync::mpsc::{self, channel},
    time::Duration,
};
use tracing::{info, trace, warn};

pub fn print(msg: String) -> JsValue {
    info!("api-print: [{msg}]");
    JsValue::Null
}

pub fn assert_script_run_ssh_seperate(cmd: String, timeout: i32) -> Option<String> {
    trace!("assert_script_run::req");
    let msg_tx = get_global_sender();
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
        Some(res)
    } else {
        None
    };
    trace!("assert_script_run done");
    res
}

pub fn assert_script_run_ssh_global(cmd: String, timeout: i32) -> Option<String> {
    trace!("assert_script_run::req");
    let msg_tx = get_global_sender();
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
        Some(res)
    } else {
        None
    };
    trace!("assert_script_run done");
    res
}

pub fn assert_script_run_serial_global(cmd: String, timeout: i32) -> (bool, String) {
    trace!("assert_script_run::req");
    let msg_tx = get_global_sender();
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

    trace!("assert_script_run send done, waiting...");
    let res = rx.recv().unwrap();

    let res = if let MsgRes::AssertScriptRunSerialGlobal { res } = res {
        (true, res)
    } else {
        (false, "".to_string())
    };
    trace!("assert_script_run done");
    res
}

pub fn assert_screen(tag: String, timeout: i32) -> Result<bool, ()> {
    let msg_tx = get_global_sender();
    let (tx, rx) = channel::<MsgRes>();

    msg_tx
        .send((
            MsgReq::AssertScreen {
                tag,
                threshold: 1,
                timeout: Duration::from_millis(timeout as u64),
            },
            tx,
        ))
        .unwrap();

    let res = rx.recv_timeout(Duration::from_millis(timeout as u64));
    let res = match res {
        Ok(MsgRes::AssertScreen { similarity: 0, ok }) => Ok(ok),
        Ok(res) => {
            // wrong msg type
            panic!("msg handler receive error type: [{:?}]", res);
        }
        Err(_) => Ok(false), // timeout
    };
    trace!(msg = "assert_screen done");
    res
}

pub fn mouse_click() {
    let msg_tx = get_global_sender();
    let (tx, rx) = channel::<MsgRes>();

    msg_tx.send((MsgReq::MouseClick, tx)).unwrap();
    let res = rx.recv().unwrap();
    let res = if let MsgRes::Done = res {
        Ok(())
    } else {
        Err(())
    };
    trace!(msg = "mouse_click done");
}

pub fn mouse_hide() {
    let msg_tx = get_global_sender();
    let (tx, rx) = channel::<MsgRes>();

    msg_tx.send((MsgReq::MouseHide, tx)).unwrap();
    let res = rx.recv().unwrap();
    let res = if let MsgRes::Done = res {
        Ok(())
    } else {
        Err(())
    };
    trace!(msg = "mouse_click done");
}

pub fn mouse_move(x: u16, y: u16) {
    info!(msg = "mouse move");
    let msg_tx = get_global_sender();
    let (tx, rx) = channel::<MsgRes>();

    msg_tx.send((MsgReq::MouseMove { x, y }, tx)).unwrap();
    let res = rx.recv().unwrap();
    match res {
        MsgRes::Done => {
            info!(msg = "mouse move done");
        }
        _ => {
            warn!(msg = "mouse move failed");
        }
    };
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
