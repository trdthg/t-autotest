use super::text_console::BufEvLoopCtl;
use crate::{term::Term, EvLoopCtl, Req};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use t_util::ExecutorError;
use tracing::info;

pub struct SerialClient<T: Term> {
    pub ctl: BufEvLoopCtl<T>,
    pub tty: String,
}

#[derive(Debug)]
pub enum SerialError {
    ConnectionBroken,
    Timeout,
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

type Result<T> = std::result::Result<T, SerialError>;

impl<T> SerialClient<T>
where
    T: Term,
{
    pub fn connect(file: &str, bund_rate: u32, auth: Option<(&str, &str)>) -> Result<Self> {
        let path = Path::new(file);
        if !path.exists() {
            panic!("serial path not exists");
        }

        // init tty
        // t_util::execute_shell(
        //     format!("stty -F {} {} -echo -icrnl -onlcr -icanon", file, bund_rate).as_str(),
        // )
        // .map_err(SerialError::Stty)?;

        // connect serial
        let port = serialport::new(file, bund_rate)
            .open()
            .map_err(|_| SerialError::ConnectionBroken)
            .unwrap();

        let mut res = Self {
            ctl: BufEvLoopCtl::new(EvLoopCtl::new(port)),
            tty: "".to_string(),
        };

        if let Some((username, password)) = auth {
            info!(msg = "serial try logout");
            if let Err(_e) = res.logout() {
                panic!("serial try logout failed");
            };

            info!(msg = "serial waiting login");
            if let Err(e) = res
                .ctl
                .wait_string_ntimes(Duration::from_secs(30), "login", 1)
            {
                panic!("serial login wait prompt failed: {}", e);
            };

            info!(msg = "serial login");
            res.login(username.as_ref(), password.as_ref());
        }

        info!(msg = "serial get tty");
        if let Ok(tty) = res.ctl.exec_global(Duration::from_secs(10), "tty") {
            res.tty = tty.1;
        } else {
            panic!("serial get tty failed")
        }

        Ok(res)
    }

    pub fn tty(&self) -> String {
        self.tty.to_owned()
    }

    fn logout(&mut self) -> Result<()> {
        // logout
        self.ctl
            .write(b"\x04\n")
            .map_err(|_e| SerialError::ConnectionBroken)?;
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
        T::parse_and_strip(&self.ctl.history())
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        sleep(Duration::from_millis(100));
        self.ctl.write_string(s).map_err(|_| SerialError::Timeout)?;
        Ok(())
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
                    .ctl
                    .exec_global(Duration::from_secs(1), cmd.0)
                    .unwrap();
                assert_eq!(res.0, 0);
                assert_eq!(res.1, cmd.1);
            }
        })
    }
}
