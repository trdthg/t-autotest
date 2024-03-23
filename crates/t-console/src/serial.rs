use crate::base::evloop::{EvLoopCtl, Req};
use crate::base::tty::Tty;
use crate::term::Term;
use crate::ConsoleError;
use std::path::PathBuf;
use std::{thread::sleep, time::Duration};
use tracing::error;

pub struct Serial {
    _config: t_config::ConsoleSerial,
    inner: SerialClient<crate::VT102>,
    history: String,
}

type Result<T> = std::result::Result<T, ConsoleError>;

impl Serial {
    pub fn new(c: t_config::ConsoleSerial) -> Result<Self> {
        let inner = Self::connect_from_serial_config(&c)?;

        Ok(Self {
            _config: c,
            inner,
            history: String::new(),
        })
    }

    pub fn stop(&self) {
        if self.inner.tty.send(Req::Stop).is_err() {
            error!("serial evloop stopped failed");
        }
    }

    pub fn reconnect(&mut self) -> Result<()> {
        self.history.push_str(self.inner.history().as_str());
        self.inner = Self::connect_from_serial_config(&self._config)?;
        Ok(())
    }

    fn connect_from_serial_config(
        c: &t_config::ConsoleSerial,
    ) -> Result<SerialClient<crate::VT102>> {
        let ssh_client = SerialClient::connect(&c.serial_file, c.bund_rate, c.log_file.clone())?;
        Ok(ssh_client)
    }

    pub fn tty(&self) -> String {
        self.inner.path.clone()
    }

    pub fn history(&mut self) -> String {
        self.history.push_str(self.inner.history().as_str());
        self.inner.history()
    }

    fn do_with_reconnect<T>(
        &mut self,
        f: impl Fn(&mut Self) -> std::result::Result<T, crate::ConsoleError>,
    ) -> Result<T> {
        let mut retry = 3;
        loop {
            retry -= 1;
            if retry == 0 {
                return Err(ConsoleError::Timeout);
            }

            match f(self) {
                Ok(v) => return Ok(v),
                Err(e) => match e {
                    crate::ConsoleError::ConnectionBroken(_) => {
                        let _ = self.reconnect();
                        continue;
                    }
                    _ => {
                        return Err(ConsoleError::Timeout);
                    }
                },
            }
        }
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        self.do_with_reconnect(|c| c.inner.tty.write_string(s))?;
        Ok(())
    }

    pub fn exec_global(&mut self, timeout: Duration, cmd: &str) -> Result<(i32, String)> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        sleep(Duration::from_millis(70));
        self.do_with_reconnect(|c| c.inner.tty.exec_global(timeout, cmd))
    }

    pub fn wait_string_ntimes(
        &mut self,
        timeout: Duration,
        pattern: &str,
        repeat: usize,
    ) -> Result<String> {
        self.do_with_reconnect(|c| c.inner.tty.wait_string_ntimes(timeout, pattern, repeat))
    }
}

struct SerialClient<T: Term> {
    pub tty: Tty<T>,
    pub path: String,
}

impl<T> SerialClient<T>
where
    T: Term,
{
    pub fn connect(file: &str, bund_rate: u32, log_file: Option<PathBuf>) -> Result<Self> {
        // init tty
        // t_util::execute_shell(
        //     format!("stty -F {} {} -echo -icrnl -onlcr -icanon", file, bund_rate).as_str(),
        // )
        // .map_err(ConsoleError::Stty)?;

        // connect serial
        let port = serialport::new(file, bund_rate).open().map_err(|e| {
            error!("{}", e);
            ConsoleError::ConnectionBroken(format!("serialport open failed, {}", e))
        })?;

        Ok(Self {
            tty: Tty::new(EvLoopCtl::new(port, log_file)),
            path: "".to_string(),
        })
    }

    #[allow(unused)]
    pub fn tty(&self) -> String {
        self.path.to_owned()
    }

    pub fn history(&self) -> String {
        T::parse_and_strip(&self.tty.history())
    }
}

#[cfg(test)]
mod test {
    use t_config::{Config, ConsoleSerial};

    use crate::term::{Term, VT102};
    use std::{
        env,
        io::{ErrorKind, Read},
        thread::sleep,
        time::Duration,
    };

    use super::SerialClient;

    #[test]
    fn test_serial_boot() {
        let c = get_config_from_file();
        if c.is_none() {
            return;
        }
        let c = c.unwrap();
        let Some(serial) = c.serial else {
            return;
        };

        let port = serialport::new(serial.serial_file, serial.bund_rate)
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
                    println!("{}", VT102::parse_and_strip(&buf[0..n]));
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

    fn get_client(serial: &ConsoleSerial) -> SerialClient<VT102> {
        SerialClient::connect(&serial.serial_file, serial.bund_rate, None).unwrap()
    }

    #[test]
    fn test_exec_global() {
        let Some(c) = get_config_from_file() else {
            return;
        };
        let Some(c) = c.serial else {
            return;
        };
        let mut serial = get_client(&c);

        let cmds = [
            ("unset A", ""),
            (r#"echo "A=$A""#, "A=\n"),
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=1\n"),
        ];

        (0..10).for_each(|_| {
            for cmd in cmds.iter() {
                let res = serial
                    .tty
                    .exec_global(Duration::from_secs(1), cmd.0)
                    .unwrap();
                assert_eq!(res.0, 0);
                assert_eq!(res.1, cmd.1);
            }
        })
    }
}
