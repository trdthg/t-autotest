use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::PathBuf,
    sync::mpsc::{self, channel, Receiver, Sender},
    thread,
    time::Duration,
};

use crate::{ConsoleError, Result};
use tracing::{debug, error, warn};

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
    stop_tx: Sender<()>,
}

impl EvLoopCtl {
    pub fn send_timeout(
        &self,
        req: Req,
        timeout: Duration,
    ) -> std::result::Result<Res, mpsc::RecvTimeoutError> {
        let (tx, rx) = channel();
        if let Err(e) = self.req_tx.send((req, tx)) {
            error!("evloop receiver closed, connection may be lost: {}", e);
            return Err(mpsc::RecvTimeoutError::Disconnected);
        }
        rx.recv_timeout(timeout)
    }

    pub fn stop(&self) {
        if self.stop_tx.send(()).is_err() {
            error!("evloop closed");
        }
    }
}

pub struct EventLoop<T> {
    make_conn: Box<dyn Fn() -> Result<T>>,
    conn: Option<T>,
    req_rx: Receiver<(Req, Sender<Res>)>,
    stop_rx: Receiver<()>,
    history: Vec<u8>,
    log_file: Option<File>,
    last_read_index: usize,
    buffer: Vec<u8>,
}

impl<T> EventLoop<T>
where
    T: Read + Write + Send + 'static,
{
    pub fn spawn(
        make_conn: impl Fn() -> Result<T> + Send + 'static,
        log_file: Option<PathBuf>,
    ) -> Result<EvLoopCtl> {
        let conn = make_conn()?;

        let log_file = if let Some(ref log_file) = log_file {
            let file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(log_file)
                .expect("Failed to open file");
            Some(file)
        } else {
            None
        };

        let (req_tx, req_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();

        thread::spawn(move || {
            Self {
                conn: Some(conn),
                make_conn: Box::new(make_conn),
                req_rx,
                stop_rx,
                log_file,
                history: Vec::new(),
                last_read_index: 0,
                buffer: vec![0u8; 4096],
            }
            .pool();
        });
        Ok(EvLoopCtl { req_tx, stop_tx })
    }

    fn pool(&mut self) {
        'out: loop {
            if self.stop_rx.try_recv().is_ok() {
                break 'out;
            }

            // handle tty output
            if let Err(e) = self.try_read_buffer() {
                error!(msg="connection lost", reason = ?e);
                break 'out;
            }

            thread::sleep(Duration::from_millis(10));

            // handle user read, write request
            match self.req_rx.try_recv() {
                Ok((req, tx)) => {
                    // handle stop
                    // block until receive new buffer, try receive only once
                    let res = match req {
                        Req::Write(msg) => {
                            if let Err(e) = self.write_buffer(&msg) {
                                error!(msg="connection lost", reason = ?e);
                                break 'out;
                            }
                            debug!(msg = "write done");
                            Res::Done
                        }
                        Req::Read => Res::Value(self.consume_buffer()),
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

    fn try_read_buffer(&mut self) -> Result<Vec<u8>> {
        'out: loop {
            match &mut self.conn {
                Some(conn) => match conn.read(&mut self.buffer) {
                    Ok(n) => {
                        if n == 0 {
                            return Ok(Vec::new());
                        }
                        let received = &self.buffer[0..n];
                        self.history.extend(received);

                        if let Some(ref mut log_file) = self.log_file {
                            if let Err(e) = log_file.write_all(received) {
                                warn!(msg = "unable write to log", reason = ?e);
                                self.log_file = None;
                            }
                        }
                        return Ok(received.to_vec());
                    }
                    Err(e) => match e.kind() {
                        io::ErrorKind::ConnectionRefused
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::BrokenPipe => {
                            // drop conn, relese fd, release /dev/ttyUSB0
                            self.conn = None;
                            continue;
                        }
                        io::ErrorKind::TimedOut => return Ok(Vec::new()),
                        _ => {
                            error!(msg = "read failed, connection may be broken", reason = ?e);
                            return Err(ConsoleError::IO(e));
                        }
                    },
                },
                None => loop {
                    if let Ok(conn) = self.make_conn.as_mut()() {
                        self.conn = Some(conn);
                    } else {
                        thread::sleep(Duration::from_millis(20));
                    }
                    continue 'out;
                },
            };
        }
    }

    fn write_buffer(&mut self, bytes: &[u8]) -> Result<()> {
        'out: loop {
            match self.conn.as_mut() {
                Some(conn) => {
                    if let Err(e) = conn.write_all(bytes) {
                        match e.kind() {
                            io::ErrorKind::ConnectionRefused
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::BrokenPipe => {
                                self.conn = None;
                                continue;
                            }
                            io::ErrorKind::TimedOut => continue,
                            _ => {
                                error!(msg = "write failed, connection may be broken", reason = ?e);
                                return Err(ConsoleError::IO(e));
                            }
                        }
                    }
                    if let Err(e) = conn.flush() {
                        match e.kind() {
                            io::ErrorKind::ConnectionRefused
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::BrokenPipe => {
                                self.conn = None;
                                continue;
                            }
                            io::ErrorKind::TimedOut => continue,
                            _ => {
                                error!(msg = "flush failed, connection may be broken", reason = ?e);
                                return Err(ConsoleError::IO(e));
                            }
                        }
                    }
                    return Ok(());
                }
                None => {
                    if let Ok(conn) = self.make_conn.as_mut()() {
                        self.conn = Some(conn);
                    } else {
                        thread::sleep(Duration::from_millis(20));
                    }
                    continue 'out;
                }
            }
        }
    }

    fn consume_buffer(&mut self) -> Vec<u8> {
        if self.last_read_index == self.history.len() {
            return Vec::new();
        }
        let res = &self.history[self.last_read_index..];
        self.last_read_index = self.history.len();
        res.to_vec()
    }
}
