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
use tracing::{debug, info};

pub struct SerialClient {
    ctl: BufEvLoopCtl,
}

impl DuplexChannelConsole for SerialClient {}

#[derive(Debug)]
pub enum SerialError {
    ConnectError(String),
    Read(io::Error),
    Write(io::Error),
    Stty(ExecutorError),
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
        .map_err(SerialError::Stty)?;

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
        if let Err(e) = res {
            Err(e)
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

    pub fn exec_global(&mut self, timeout: Duration, cmd: &str) -> Result<(i32, String)> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        sleep(Duration::from_millis(70));

        let nanoid = nanoid::nanoid!();
        let cmd = format!("{cmd}; echo $?{nanoid}\n",);
        self.write(cmd.as_bytes()).unwrap();

        self.ctl.comsume_buffer_and_map(timeout, |buffer| {
            // find target pattern from buffer
            let parsed_str = parse_str_from_vt100_bytes(buffer);
            let catched_output =
                t_util::assert_capture_between(&parsed_str, &format!("{nanoid}\n"), &nanoid)
                    .unwrap();
            match catched_output {
                Some(v) => {
                    info!(
                        msg = "catched_output",
                        nanoid = nanoid,
                        parsed_str = parsed_str,
                    );
                    if let Some((res, flag)) = v.rsplit_once('\n') {
                        info!(nanoid = nanoid, flag = flag, res = res);
                        if let Ok(flag) = flag.parse::<i32>() {
                            return Some((flag, res.to_string()));
                        }
                    } else {
                        // some command doesn't print, like 'sleep'
                        if let Ok(flag) = v.parse::<i32>() {
                            return Some((flag, "".to_string()));
                        }
                    }
                    Some((1, v))
                }
                // means continue
                None => {
                    debug!(
                        msg = "current bufferw",
                        nanoid = nanoid,
                        parsed_str = parsed_str
                    );
                    None
                }
            }
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
        let f = env::var("AUTOTEST_CONFIG_FILE").ok();
        f.as_ref()?;
        let c = t_config::load_config_from_file(f.unwrap()).map(Some);
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

        SerialClient::connect(&c.serial_file, c.bund_rate, auth).unwrap()
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

        let cmds = [
            ("unset A", ""),
            (r#"echo "A=$A""#, "A=\n"),
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=1\n"),
        ];

        (0..10).for_each(|_| {
            for cmd in cmds.iter() {
                trace!(cmd = cmd.0);
                let res = serial.exec_global(Duration::from_secs(1), cmd.0).unwrap();
                assert_eq!(res.0, 0);
                assert_eq!(res.1, cmd.1);
            }
        })
    }
}
