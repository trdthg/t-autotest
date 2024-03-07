use crate::base::evloop::{EvLoopCtl, Req};
use crate::base::tty::Tty;
use crate::term::Term;
use crate::ConsoleError;
use std::{thread::sleep, time::Duration};
use tracing::error;
use tracing::{debug, info};

pub struct SerialTty {
    _config: t_config::ConsoleSerial,
    inner: SerialClient<crate::VT102>,
    history: String,
}

type Result<T> = std::result::Result<T, ConsoleError>;

impl SerialTty {
    pub fn new(c: t_config::ConsoleSerial) -> Self {
        let inner = Self::connect_from_serial_config(&c);

        Self {
            _config: c,
            inner,
            history: String::new(),
        }
    }

    pub fn reconnect(&mut self) {
        self.history.push_str(self.inner.history().as_str());
        self.inner = Self::connect_from_serial_config(&self._config)
    }

    fn connect_from_serial_config(c: &t_config::ConsoleSerial) -> SerialClient<crate::VT102> {
        if !c.enable {
            panic!("serial is disabled in config");
        }
        info!(msg = "init ssh...");
        let username = c.username.clone().unwrap_or_default();
        let password = c.password.clone().unwrap_or_default();
        let auth = match c.auto_login {
            true => Some((username.as_str(), password.as_str())),
            false => None,
        };
        let ssh_client = SerialClient::connect(&c.serial_file, c.bund_rate, auth)
            .unwrap_or_else(|_| panic!("init ssh connection failed: {:?}", auth));
        info!(msg = "init ssh done");
        ssh_client
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
                    crate::ConsoleError::ConnectionBroken => {
                        self.reconnect();
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

impl<T> Drop for SerialClient<T>
where
    T: Term,
{
    fn drop(&mut self) {
        // try logout
        self.tty.send(Req::Write(vec![0x04])).unwrap();
    }
}

impl<T> SerialClient<T>
where
    T: Term,
{
    pub fn connect(file: &str, bund_rate: u32, auth: Option<(&str, &str)>) -> Result<Self> {
        // init tty
        // t_util::execute_shell(
        //     format!("stty -F {} {} -echo -icrnl -onlcr -icanon", file, bund_rate).as_str(),
        // )
        // .map_err(ConsoleError::Stty)?;

        // connect serial
        let port = serialport::new(file, bund_rate)
            .open()
            .map_err(|e| {
                error!("{}", e);
                ConsoleError::ConnectionBroken
            })
            .expect("connect to serialport failed");

        let mut res = Self {
            tty: Tty::new(EvLoopCtl::new(port)),
            path: "".to_string(),
        };

        if let Some((username, password)) = auth {
            info!(msg = "serial try logout");
            if let Err(_e) = res.logout() {
                panic!("serial try logout failed");
            };

            info!(msg = "serial waiting login");
            if let Err(e) = res
                .tty
                .wait_string_ntimes(Duration::from_secs(30), "login", 1)
            {
                panic!("serial login wait prompt failed: {}", e);
            };

            info!(msg = "serial login");
            res.login(username.as_ref(), password.as_ref());

            info!(msg = "serial get tty");
            if let Ok(tty) = res.tty.exec_global(Duration::from_secs(10), "tty") {
                res.path = tty.1;
            } else {
                panic!("serial get tty failed")
            }
        }

        Ok(res)
    }

    pub fn tty(&self) -> String {
        self.path.to_owned()
    }

    fn logout(&mut self) -> Result<()> {
        // logout
        self.tty
            .write(b"\x04\n")
            .map_err(|_e| ConsoleError::ConnectionBroken)?;
        sleep(Duration::from_millis(5000));
        Ok(())
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

    pub fn history(&self) -> String {
        T::parse_and_strip(&self.tty.history())
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        sleep(Duration::from_millis(100));
        self.tty
            .write_string(s)
            .map_err(|_| ConsoleError::Timeout)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use t_config::Config;
    use tracing::trace;

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

    fn get_client(c: &Config) -> SerialClient<VT102> {
        assert!(c.console.serial.enable);

        let c = c.console.serial.clone();
        let username = c.username.unwrap_or_default();
        let password = c.password.unwrap_or_default();
        let auth = match c.auto_login {
            true => Some((username.as_str(), password.as_str())),
            false => None,
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
