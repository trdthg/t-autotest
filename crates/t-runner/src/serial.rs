use std::{thread::sleep, time::Duration};

use t_console::SerialError;
use tracing::{debug, info};

pub struct SerialClient {
    _config: t_config::ConsoleSerial,
    inner: t_console::SerialClient<t_console::VT102>,
    history: String,
}

impl SerialClient {
    pub fn new(c: t_config::ConsoleSerial) -> Self {
        let mut inner = Self::connect_from_serial_config(&c);

        debug!(msg = "ssh getting tty...");
        let Ok((code, tty)) = inner.ctl.exec_global(Duration::from_secs(10), "tty") else {
            panic!("ssh get tty failed");
        };
        if code != 0 {
            panic!("get tty failed");
        }
        inner.tty = tty;
        info!(msg = "ssh client tty", tty = inner.tty.trim());

        Self {
            _config: c,
            inner,
            history: String::new(),
        }
    }

    pub fn _reconnect(&mut self) {
        self.history.push_str(self.inner.history().as_str());
        self.inner = Self::connect_from_serial_config(&self._config)
    }

    fn connect_from_serial_config(
        c: &t_config::ConsoleSerial,
    ) -> t_console::SerialClient<t_console::VT102> {
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
        let ssh_client = t_console::SerialClient::connect(&c.serial_file, c.bund_rate, auth)
            .unwrap_or_else(|_| panic!("init ssh connection failed: {:?}", auth));
        info!(msg = "init ssh done");
        ssh_client
    }

    pub fn tty(&self) -> String {
        self.inner.tty.clone()
    }

    pub fn history(&mut self) -> String {
        self.history.push_str(self.inner.history().as_str());
        self.inner.history()
    }

    pub fn write_string(&mut self, s: &str) -> Result<(), SerialError> {
        self.inner
            .ctl
            .write_string(s)
            .map_err(|_| SerialError::Timeout)?;
        Ok(())
    }

    pub fn exec_global(
        &mut self,
        timeout: Duration,
        cmd: &str,
    ) -> Result<(i32, String), SerialError> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        sleep(Duration::from_millis(70));

        self.inner
            .ctl
            .exec_global(timeout, cmd)
            .map_err(|_| SerialError::Timeout)
    }

    pub fn wait_string_ntimes(
        &mut self,
        timeout: Duration,
        pattern: &str,
        repeat: usize,
    ) -> Result<String, SerialError> {
        self.inner
            .ctl
            .wait_string_ntimes(timeout, pattern, repeat)
            .map_err(|_| SerialError::Timeout)
    }
}
