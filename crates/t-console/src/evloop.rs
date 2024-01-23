use std::{
    io::{self, Read, Write},
    sync::mpsc::{self, channel, Receiver, Sender},
    thread,
};

use anyhow::Result;
use image::EncodableLayout;
use tracing::error;

mod serial;
mod ssh;
mod text_console;

pub use serial::*;
pub use ssh::*;

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

pub struct EvLoopCtl {
    req_tx: Sender<(Req, Sender<Res>)>,
}

impl EvLoopCtl {
    pub fn new<T: Read + Write + Send + 'static>(conn: T) -> Self {
        let req_tx = EventLoop::spawn(conn);
        Self { req_tx }
    }

    fn send(&self, req: Req) -> Result<Res, mpsc::RecvError> {
        let (tx, rx) = channel();
        self.req_tx.send((req, tx)).unwrap();
        let res = rx.recv()?;
        Ok(res)
    }
}

struct EventLoop<T> {
    conn: T,
    read_rx: Receiver<(Req, Sender<Res>)>,
    stop_tx: Sender<()>,
    stop_rx: Receiver<()>,
    buffer: Vec<u8>,
    last_read_index: usize,
}

impl<T> Drop for EventLoop<T> {
    fn drop(&mut self) {
        self.stop_tx.send(()).unwrap()
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
                buffer: Vec::new(),
                last_read_index: 0,
            }
            .pool();
        });
        write_tx
    }

    fn pool(&mut self) {
        let mut output_buffer = [0u8; 4096];
        loop {
            // handle stop
            if let Ok(()) = self.stop_rx.try_recv() {
                break;
            }

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
                    break;
                }
            }
        }
    }
}
