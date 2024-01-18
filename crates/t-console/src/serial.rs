use super::DuplexChannelConsole;
use crate::event_loop::{BufEvLoopCtl, Ctl};
use crate::{parse_str_from_vt100_bytes, BufCtl, EvLoopCtl, Req, Res};

use anyhow::Result;

use std::io;
use std::path::Path;
use std::sync::mpsc;
use std::thread::sleep;
use std::time::Duration;

use t_util::ExecutorError;
use tracing::{debug, info, trace};

pub struct SerialClient {
    ctl: BufEvLoopCtl,
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
        self.ctl.send(Req::Write(vec![0x04])).unwrap();
        self.ctl.stop();
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

        // connect serial
        let port = serialport::new(&file, bund_rate)
            .open()
            .map_err(|e| SerialError::ConnectError(e.to_string()))
            .unwrap();

        let mut res = Self {
            ctl: BufEvLoopCtl::new(EvLoopCtl::new(port)),
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
        parse_str_from_vt100_bytes(&self.ctl.history())
    }

    fn write(&self, bytes: &[u8]) -> Result<(), mpsc::RecvError> {
        let res = self.ctl.send(Req::Write(bytes.to_vec()));
        if res.is_err() {
            Err(res.unwrap_err())
        } else {
            assert!(matches!(res, Ok(Res::Done)));
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
        let cmd = format!("{cmd}; echo {nanoid}\n",);
        self.write(cmd.as_bytes()).unwrap();

        self.ctl.comsume_buffer_and_map(timeout, |buffer| {
            // find target pattern from buffer
            let parsed_str = parse_str_from_vt100_bytes(buffer);
            let res = t_util::assert_capture_between(&parsed_str, &format!("{nanoid}\n"), &nanoid)
                .unwrap();
            trace!(nanoid = nanoid, parsed_str = parsed_str);
            res
        })
    }

    pub fn read_golbal_until(&mut self, timeout: Duration, pattern: &str) -> Result<()> {
        self.ctl
            .comsume_buffer_and_map(timeout, |buffer| {
                let buffer_str = parse_str_from_vt100_bytes(buffer);
                debug!(msg = "serial read_golbal_until", buffer = buffer_str);
                buffer_str.find(pattern)
            })
            .map(|_| ())
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
        let port = serialport::new("/dev/ttyUSB0", 115200)
            .timeout(Duration::from_secs(10))
            .open_native();
        if port.is_err() {
            return;
        }
        let mut port = port.unwrap();
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

    fn get_config_from_file() -> Option<Config> {
        let f = env::var("AUTOTEST_CONFIG_FILE").map_or(None, |v| Some(v));
        if f.is_none() {
            return None;
        }
        let c = t_config::load_config_from_file(f.unwrap()).map(|v| Some(v));
        c.unwrap()
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
        if c.is_none() {
            return;
        }
        let c = c.unwrap();
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
