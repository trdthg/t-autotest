use crate::{term::Term, EvLoopCtl, Req};

use anyhow::Result;

use std::io;
use std::path::Path;

use std::thread::sleep;
use std::time::Duration;

use t_util::ExecutorError;
use tracing::{debug, info};

use super::text_console::BufEvLoopCtl;

pub struct SerialClient<T: Term> {
    ctl: BufEvLoopCtl<T>,
    tty: String,
}

#[derive(Debug)]
pub enum SerialError {
    ConnectError(String),
    Read(io::Error),
    Write(io::Error),
    Stty(ExecutorError),
}

impl<T> Drop for SerialClient<T>
where
    T: Term,
{
    fn drop(&mut self) {
        // try logout
        self.ctl.send(Req::Write(vec![0x04])).unwrap();
    }
}

impl<T> SerialClient<T>
where
    T: Term,
{
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
            tty: "".to_string(),
        };
        res.logout();

        res.read_golbal_until(Duration::from_secs(30), "login")
            .unwrap();
        if let Some((username, password)) = auth {
            res.login(&username.into(), &password.into());
        }

        if let Ok(tty) = res.exec_global(Duration::from_secs(10), "tty") {
            res.tty = tty.1;
        } else {
            panic!("serial basic ")
        }

        Ok(res)
    }

    pub fn dump_history(&self) -> String {
        T::parse(&self.ctl.history())
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        sleep(Duration::from_millis(100));
        self.ctl.write_string(s)?;
        Ok(())
    }

    fn logout(&mut self) {
        // logout
        self.ctl.write(b"\x04\n");
        sleep(Duration::from_millis(5000));
    }

    fn login(&mut self, username: &str, password: &str) {
        // username
        self.write_string(&format!("{username}\n")).unwrap();
        sleep(Duration::from_millis(5000));

        // password
        self.write_string(&format!("{password}\n")).unwrap();
        sleep(Duration::from_millis(3000));

        info!("{}", "try login done");
    }

    pub fn exec_global(&mut self, timeout: Duration, cmd: &str) -> Result<(i32, String)> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        sleep(Duration::from_millis(70));

        self.ctl.exec_global(timeout, cmd)
    }

    pub fn read_golbal_until(&mut self, timeout: Duration, pattern: &str) -> Result<()> {
        self.ctl
            .comsume_buffer_and_map(timeout, |buffer| {
                let buffer_str = T::parse(buffer);
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

    use crate::{
        term::{Term, VT102},
        SerialClient,
    };
    use std::{
        env,
        io::{ErrorKind, Read},
        thread::sleep,
        time::Duration,
    };

    #[test]
    fn test_serial_boot() {
        let c = get_config_from_file();
        if c.is_none() {
            return;
        }
        let c = c.unwrap();
        if !c.console.serial.enable {
            return;
        }

        let port = serialport::new(c.console.serial.serial_file, c.console.serial.bund_rate)
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
                    println!("{}", VT102::parse(&buf[0..n]));
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

    fn get_client(c: &Config) -> SerialClient<VT102> {
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
