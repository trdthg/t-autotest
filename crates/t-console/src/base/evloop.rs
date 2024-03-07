use std::{
    io::{self, Read, Write},
    sync::mpsc::{self, channel, Receiver, Sender},
    thread,
    time::Duration,
};

use image::EncodableLayout;
use tracing::{debug, error, info, warn};

#[derive(Debug)]
pub enum Req {
    Write(Vec<u8>),
    Read,
    Dump,
    #[allow(unused)]
    Stop,
}

#[derive(Debug)]
pub enum Res {
    Done,
    Value(Vec<u8>),
}

pub struct EvLoopCtl {
    req_tx: Sender<(Req, Sender<Res>)>,
}

impl EvLoopCtl {
    pub fn new<T: Read + Write + Send + 'static>(conn: T) -> Self {
        let req_tx = EventLoop::spawn(conn);
        Self { req_tx }
    }

    pub fn send(&self, req: Req) -> Result<Res, mpsc::RecvError> {
        let (tx, rx) = channel();
        if let Err(e) = self.req_tx.send((req, tx)) {
            error!("evloop receiver closed, connection may be lost: {}", e);
            return Err(mpsc::RecvError {});
        }
        rx.recv()
    }

    pub fn send_timeout(&self, req: Req, timeout: Duration) -> Result<Res, mpsc::RecvTimeoutError> {
        let (tx, rx) = channel();
        if let Err(e) = self.req_tx.send((req, tx)) {
            error!("evloop receiver closed, connection may be lost: {}", e);
            return Err(mpsc::RecvTimeoutError::Disconnected);
        }
        rx.recv_timeout(timeout)
    }
}

struct EventLoop<T> {
    conn: T,
    read_rx: Receiver<(Req, Sender<Res>)>,
    stop_tx: Sender<()>,
    stop_rx: Receiver<()>,
    history: Vec<u8>,
    last_read_index: usize,
    buffer: Vec<u8>,
}

impl<T> Drop for EventLoop<T> {
    fn drop(&mut self) {
        if let Err(e) = self.stop_tx.send(()) {
            error!("evloop may already been dropped: {}", e);
        }
    }
}

impl<T> EventLoop<T>
where
    T: Read + Write + Send + 'static,
{
    pub fn spawn(conn: T) -> Sender<(Req, Sender<Res>)> {
        let (write_tx, read_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();

        thread::spawn(move || {
            Self {
                conn,
                read_rx,
                stop_tx,
                stop_rx,
                history: Vec::new(),
                last_read_index: 0,
                buffer: vec![0u8; 4096],
            }
            .pool();
        });
        write_tx
    }

    fn pool(&mut self) {
        'out: loop {
            // handle stop
            if let Ok(()) = self.stop_rx.try_recv() {
                break;
            }

            // handle serial output
            match self.conn.read(&mut self.buffer) {
                Ok(n) => {
                    if n != 0 {
                        let received = &self.buffer[0..n];
                        self.history.extend(received);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    // ignore timeout
                }
                Err(e) => {
                    error!(msg = "connection may be broken", reason = ?e);
                    break;
                }
            }

            // handle user read, write request
            match self.read_rx.try_recv() {
                Ok((req, tx)) => {
                    if matches!(req, Req::Stop) {
                        break 'out;
                    }
                    let Ok(res) = self.handle_req(req, true) else {
                        info!("stopped while blocking");
                        break 'out;
                    };
                    if let Err(e) = tx.send(res) {
                        warn!("req sender side closed before recv response: {}", e);
                        continue;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // ignore empty
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    // sender closed, evloop should stop here
                    break;
                }
            }
        }
    }

    // block until receive new buffer, try receive only once
    fn handle_req(&mut self, req: Req, blocking: bool) -> Result<Res, ()> {
        match req {
            Req::Stop => {
                // should be handled before
                Ok(Res::Done)
            }
            Req::Write(msg) => {
                if let Err(e) = self.conn.write_all(msg.as_bytes()) {
                    error!(msg = "write failed, connection may be broken", reason = ?e);
                    return Err(());
                }
                if let Err(e) = self.conn.flush() {
                    error!(msg = "flush failed, connection may be broken", reason = ?e);
                }
                debug!(msg = "write done");
                Ok(Res::Done)
            }
            Req::Read => self.read_buffer(blocking).map(Res::Value),
            Req::Dump => Ok(Res::Value(self.history.clone())),
        }
    }

    fn consume_buffer(&mut self) -> Option<Vec<u8>> {
        if self.last_read_index == self.history.len() {
            return None;
        }
        let res = &self.history[self.last_read_index..];
        self.last_read_index = self.history.len();
        Some(res.to_vec())
    }

    fn read_buffer(&mut self, blocking: bool) -> Result<Vec<u8>, ()> {
        if let Some(res) = self.consume_buffer() {
            return Ok(res);
        }
        if !blocking {
            return Ok(Vec::new());
        }

        // block until receive new buffer
        debug!(msg = "blocking... try read");
        loop {
            // handle stop
            if let Ok(()) = self.stop_rx.try_recv() {
                // stop?
                return Err(());
            }

            // handle serial output
            if let Ok(n) = self.conn.read(&mut self.buffer) {
                if n != 0 {
                    let received = &self.buffer[0..n];
                    self.history.extend(received);
                    return Ok(unsafe { self.consume_buffer().unwrap_unchecked() });
                }
            }
        }
    }
}
