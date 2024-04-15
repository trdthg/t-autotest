use pyo3::Python;
use std::thread;
use std::{sync::mpsc, time::Duration};
use t_binding::api::Api;
use t_binding::error::{ApiError, Result};
use t_binding::msg::VNC;
use t_binding::{
    msg::{MsgResError, TextConsole},
    MsgReq, MsgRes,
};
use tracing::{info, trace, warn, Level};

pub(crate) struct PyApi<'a> {
    tx: &'a mpsc::Sender<(MsgReq, mpsc::Sender<MsgRes>)>,
    py: Python<'a>,
}

impl<'a> PyApi<'a> {
    pub fn new(tx: &'a mpsc::Sender<(MsgReq, mpsc::Sender<MsgRes>)>, py: Python<'a>) -> Self {
        Self { tx, py }
    }
}

impl<'a> Api for PyApi<'a> {
    fn tx(&self) -> &mpsc::Sender<(MsgReq, mpsc::Sender<MsgRes>)> {
        self.tx
    }

    fn req(&self, req: MsgReq) -> Result<MsgRes> {
        let msg_tx = self.tx();

        trace!(msg = "sending req");
        let (tx, rx) = mpsc::channel::<MsgRes>();
        msg_tx
            .send((req, tx))
            .map_err(|_| ApiError::ServerStopped)?;

        trace!(msg = "waiting res");
        loop {
            match rx.try_recv() {
                Ok(res) => {
                    trace!(msg = "received res");
                    return Ok(res);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => return Err(ApiError::ServerStopped),
            }
            self.py.check_signals().map_err(|_| ApiError::Interrupt)?;
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn sleep(&self, secs: u64) {
        for i in 0..secs {
            std::thread::sleep(Duration::from_secs(1));
            self.py.check_signals();
        }
    }
}
