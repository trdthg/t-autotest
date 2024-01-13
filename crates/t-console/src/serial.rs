use super::DuplexChannelConsole;
use crate::parse_str_from_vt100_bytes;
use anyhow::Result;
use image::EncodableLayout;
use serialport::TTYPort;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, channel, Receiver, Sender};
use std::thread::{self, sleep};
use std::time::{self, Duration};
use t_util::ExecutorError;
use tracing::{debug, error, info, trace};

pub struct SerialClient {
    buffer: Vec<u8>,
    history: Vec<u8>,
    tx: Sender<(MsgReq, Sender<MsgRes>)>,
    stop_tx: Sender<()>,
}

#[derive(Debug)]
enum MsgReq {
    Write(Vec<u8>),
    Read,
}

#[derive(Debug)]
enum MsgRes {
    Done,
    ReadResponse(Vec<u8>),
}

impl DuplexChannelConsole for SerialClient {}

#[derive(Debug)]
pub enum SerialError {
    ConnectError(String),
    Read(io::Error),
    Write(io::Error),
    STTY(ExecutorError),
}

impl Drop for SerialClient {
    fn drop(&mut self) {
        println!("serial client dropping...");

        // try send logout req
        let (tx, rx) = channel();
        self.tx.send((MsgReq::Write(vec![0x04]), tx)).unwrap();
        rx.recv().unwrap();

        // stop sub thread
        self.stop_tx.send(()).unwrap();
    }
}

impl SerialClient {
    pub fn connect(
        file: impl Into<String>,
        bund_rate: u32,
        auth: Option<(impl Into<String>, impl Into<String>)>,
    ) -> Result<Self, SerialError> {
        let file: String = file.into();
        let path = Path::new(&file);
        if !path.exists() {
            panic!("serial path not exists");
        }

        // init tty
        t_util::execute_shell(
            format!("stty -F {} {} -echo -icrnl -onlcr -icanon", file, bund_rate).as_str(),
        )
        .map_err(|e| SerialError::STTY(e))?;

        let port = serialport::new(&file, bund_rate)
            .open_native()
            .map_err(|e| SerialError::ConnectError(e.to_string()))?;

        let (write_tx, read_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();

        thread::spawn(move || {
            SerialClientInner::new(port, read_rx, stop_rx).pool();
        });

        let mut res = Self {
            buffer: Vec::new(),
            history: Vec::new(),
            tx: write_tx,
            stop_tx,
        };

        res.logout();

        res.read_golbal_until(Duration::from_secs(30), "login")
            .unwrap();

        if let Some((username, password)) = auth {
            res.login(&username.into(), &password.into());
        }

        Ok(res)
    }

    pub fn dump_history(&self) -> String {
        parse_str_from_vt100_bytes(&self.history)
    }

    fn write(&self, bytes: &[u8]) -> Result<(), mpsc::RecvError> {
        let (tx, rx) = channel();
        self.tx.send((MsgReq::Write(bytes.to_vec()), tx)).unwrap();
        let res = rx.recv();
        if res.is_err() {
            Err(res.unwrap_err())
        } else {
            assert!(matches!(res, Ok(MsgRes::Done)));
            Ok(())
        }
    }

    fn logout(&mut self) {
        // logout
        self.write(b"\x04\n").unwrap();
        sleep(Duration::from_millis(5000));
    }

    fn login(&mut self, username: &str, password: &str) {
        // username
        self.write(format!("{username}\n").as_bytes()).unwrap();
        sleep(Duration::from_millis(5000));

        // password
        self.write(format!("{password}\n").as_bytes()).unwrap();
        sleep(Duration::from_millis(3000));

        info!("{}", "try login done");
    }

    pub fn write_string(&self, s: &str) -> Result<(), mpsc::RecvError> {
        self.write(s.as_bytes())
    }

    pub fn exec_global(&mut self, timeout: Duration, cmd: &str) -> Result<String> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        sleep(Duration::from_millis(70));

        let nanoid = nanoid::nanoid!();
        let cmd = format!("{cmd}; echo {}\n", nanoid);
        self.write(cmd.as_bytes()).unwrap();

        self.comsume_buffer_and_map(timeout, |buffer| {
            // find target pattern from buffer
            let parsed_str = parse_str_from_vt100_bytes(buffer);
            let res = t_util::assert_capture_between(&parsed_str, &format!("{nanoid}\n"), &nanoid)
                .unwrap();
            trace!(nanoid = nanoid, parsed_str = parsed_str);
            res
        })
    }

    pub fn read_golbal_until(&mut self, timeout: Duration, pattern: &str) -> Result<()> {
        self.comsume_buffer_and_map(timeout, |buffer| {
            let buffer_str = parse_str_from_vt100_bytes(buffer);
            debug!(msg = "serial read_golbal_until", buffer = buffer_str);
            buffer_str.find(pattern)
        })
        .map(|_| ())
    }

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
            let (tx, rx) = channel();
            self.tx.send((MsgReq::Read, tx)).unwrap();
            match rx.recv() {
                Ok(MsgRes::ReadResponse(ref received)) => {
                    // save to buffer
                    if received.len() == 0 {
                        continue;
                    }

                    self.buffer.extend(received);
                    self.history.extend(received);
                    trace!(msg = "received buffer", history_len = self.history.len());

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

struct SerialClientInner {
    conn: serialport::TTYPort,
    req_rx: Receiver<(MsgReq, Sender<MsgRes>)>,
    stop_rx: Receiver<()>,
    history: Vec<u8>,
    last_read_index: usize,
}

impl SerialClientInner {
    fn new(conn: TTYPort, rx: Receiver<(MsgReq, Sender<MsgRes>)>, stop_rx: Receiver<()>) -> Self {
        Self {
            conn,
            req_rx: rx,
            stop_rx,
            history: Vec::new(),
            last_read_index: 0,
        }
    }

    fn pool(self: &mut Self) {
        let mut output_buffer = [0u8; 4096];
        loop {
            // handle serial output
            match self.conn.read(&mut output_buffer) {
                Ok(n) => {
                    let received = &output_buffer[0..n];
                    self.history.extend(received);
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {}
                Err(e) => {
                    panic!("{}", format!("{}", e));
                }
            }

            // handle user read, write request
            match self.req_rx.try_recv() {
                Ok((req, tx)) => {
                    let res = match req {
                        MsgReq::Write(msg) => {
                            self.conn.write_all(msg.as_bytes()).unwrap();
                            self.conn.flush().unwrap();
                            MsgRes::Done
                        }
                        MsgReq::Read => {
                            let res = &self.history[self.last_read_index..];
                            self.last_read_index = self.history.len();
                            MsgRes::ReadResponse(res.to_vec())
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

#[cfg(test)]
mod test {
    use t_config::Config;
    use tracing::trace;

    use crate::{parse_str_from_vt100_bytes, SerialClient};
    use std::{
        env,
        io::{ErrorKind, Read},
        thread::sleep,
        time::Duration,
    };

    #[test]
    fn test_serial_boot() {
        let mut port = serialport::new("/dev/ttyUSB0", 115200)
            .timeout(Duration::from_secs(10))
            .open_native()
            .unwrap();
        sleep(Duration::from_secs(20));
        loop {
            let mut buf = [0; 1024];
            match port.read(&mut buf) {
                Ok(n) => {
                    println!("{}", parse_str_from_vt100_bytes(&buf[0..n]));
                }
                Err(e) if e.kind() == ErrorKind::TimedOut => {
                    println!("timeout");
                }
                Err(e) => {
                    eprintln!("panic, reason: [{}]", e);
                    panic!()
                }
            }
        }
    }

    fn get_config_from_file() -> Config {
        let f = env::var("AUTOTEST_CONFIG_FILE").unwrap();
        t_config::load_config_from_file(f).unwrap()
    }

    fn get_client(c: &Config) -> SerialClient {
        assert!(c.console.serial.enable);

        let c = c.console.serial.clone();

        let auth = if c.auto_login {
            Some((c.username.unwrap(), c.password.unwrap()))
        } else {
            None
        };

        let serial = SerialClient::connect(&c.serial_file, c.bund_rate, auth).unwrap();
        serial
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_exec_global() {
        let c = get_config_from_file();
        if !c.console.serial.enable {
            return;
        }

        let mut serial = get_client(&c);

        let cmds = vec![
            ("unset A", ""),
            (r#"echo "A=$A""#, "A=\n"),
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=1\n"),
        ];

        (0..10).for_each(|_| {
            for cmd in cmds.iter() {
                trace!(cmd = cmd.0);
                let res = serial.exec_global(Duration::from_secs(1), cmd.0).unwrap();
                assert_eq!(res, cmd.1);
            }
        })
    }
}
