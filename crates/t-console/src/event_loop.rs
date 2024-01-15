use std::{
    io::{self, Read, Write},
    sync::mpsc::{self, channel, Receiver, Sender},
    thread,
    time::{self, Duration},
};

use anyhow::Result;
use image::EncodableLayout;
use tracing::{error, info};

#[derive(Debug)]
pub enum Req {
    Write(Vec<u8>),
    Read,
}

#[derive(Debug)]
pub enum Res {
    Done,
    Value(Vec<u8>),
}

pub trait Ctl {
    fn send(&self, req: Req) -> Result<Res, mpsc::RecvError>;
    fn stop(&self);
}

impl Ctl for BufEvLoopCtl {
    fn send(&self, req: Req) -> Result<Res, mpsc::RecvError> {
        self.ctl.send(req)
    }

    fn stop(&self) {
        self.ctl.stop()
    }
}

pub struct EvLoopCtl {
    req_tx: Sender<(Req, Sender<Res>)>,
    stop_tx: Sender<()>,
}

impl EvLoopCtl {
    pub fn new<T: Read + Write + Send + 'static>(conn: T) -> Self {
        let (req_tx, stop_tx) = EventLoop::new(conn);
        Self { req_tx, stop_tx }
    }
}

impl Ctl for EvLoopCtl {
    fn send(&self, req: Req) -> Result<Res, mpsc::RecvError> {
        let (tx, rx) = channel();
        self.req_tx.send((req, tx)).unwrap();
        let res = rx.recv()?;
        Ok(res)
    }

    fn stop(&self) {
        self.stop_tx.send(()).unwrap()
    }
}

struct EventLoop<T> {
    conn: T,
    read_rx: Receiver<(Req, Sender<Res>)>,
    stop_rx: Receiver<()>,
    buffer: Vec<u8>,
    last_read_index: usize,
}

impl<T> EventLoop<T>
where
    T: Read + Write + Send + 'static,
{
    pub fn new(conn: T) -> (Sender<(Req, Sender<Res>)>, Sender<()>) {
        let (write_tx, read_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();

        thread::spawn(move || {
            Self {
                conn,
                read_rx,
                stop_rx,
                buffer: Vec::new(),
                last_read_index: 0,
            }
            .pool();
        });
        (write_tx, stop_tx)
    }

    fn pool(self: &mut Self) {
        let mut output_buffer = [0u8; 4096];
        loop {
            // handle serial output
            match self.conn.read(&mut output_buffer) {
                Ok(n) => {
                    let received = &output_buffer[0..n];
                    self.buffer.extend(received);
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {}
                Err(e) => {
                    error!(msg = "connection may be broken", reason = ?e);
                    break;
                }
            }

            // handle user read, write request
            match self.read_rx.try_recv() {
                Ok((req, tx)) => {
                    let res = match req {
                        Req::Write(msg) => {
                            self.conn.write_all(msg.as_bytes()).unwrap();
                            self.conn.flush().unwrap();
                            Res::Done
                        }
                        Req::Read => {
                            let res = &self.buffer[self.last_read_index..];
                            self.last_read_index = self.buffer.len();
                            Res::Value(res.to_vec())
                        }
                    };
                    tx.send(res).unwrap();
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    error!(msg = "serial disconnected")
                }
            }

            // handle stop
            if let Ok(()) = self.stop_rx.try_recv() {
                break;
            }
        }
    }
}

pub trait BufCtl {
    fn comsume_buffer_and_map<T>(
        &mut self,
        timeout: Duration,
        f: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<T>;
}

pub struct BufEvLoopCtl {
    ctl: EvLoopCtl,
    buffer: Vec<u8>,
    history: Vec<u8>,
}

impl BufEvLoopCtl {
    pub fn new(ctl: EvLoopCtl) -> Self {
        Self {
            ctl,
            buffer: Vec::new(),
            history: Vec::new(),
        }
    }

    pub fn history(&self) -> Vec<u8> {
        self.history.clone()
    }
}

impl BufCtl for BufEvLoopCtl {
    fn comsume_buffer_and_map<T>(
        &mut self,
        timeout: Duration,
        f: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<T> {
        let current_buffer_start = self.buffer.len();

        let start = time::SystemTime::now();
        loop {
            if time::SystemTime::now().duration_since(start).unwrap() > timeout {
                break;
            }
            let res = self.ctl.send(Req::Read);
            match res {
                Ok(Res::Value(ref received)) => {
                    // save to buffer
                    if received.len() == 0 {
                        continue;
                    }

                    self.buffer.extend(received);
                    self.history.extend(received);
                    info!(
                        msg = "event loop",
                        buffer_len = received.len(),
                        history_len = self.history.len()
                    );

                    // find target pattern
                    let res = f(&self.buffer);

                    if res.is_none() {
                        continue;
                    }

                    // cut from last find
                    self.buffer = self.buffer[current_buffer_start..].to_owned();
                    return Ok(res.unwrap());
                }
                Ok(t) => {
                    error!(msg = "invalid msg varient", t = ?t);
                    panic!();
                }
                Err(e) => {
                    panic!("{}", format!("{}", e));
                }
            }
        }
        return Err(anyhow::anyhow!("timeout"));
    }
}
