use std::{io, path::Path, thread::sleep, time::Duration};

use tracing::{debug, info};

use crate::InteractError;

pub struct SSHClient {
    c: t_config::ConsoleSSH,
    inner: t_console::SSHClient<t_console::Xterm>,
    history: String,
}

impl SSHClient {
    pub fn new(c: t_config::ConsoleSSH) -> Self {
        let mut inner = Self::connect_from_ssh_config(&c);

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
            c,
            inner,
            history: "".to_string(),
        }
    }

    pub fn reconnect(&mut self) {
        self.history.push_str(self.inner.history().as_str());
        self.inner = Self::connect_from_ssh_config(&self.c)
    }

    fn connect_from_ssh_config(c: &t_config::ConsoleSSH) -> t_console::SSHClient<t_console::Xterm> {
        if !c.enable {
            panic!("ssh is disabled in config");
        }
        info!(msg = "init ssh...");
        let auth = match c.auth.r#type {
            t_config::ConsoleSSHAuthType::PrivateKey => t_console::SSHAuthAuth::PrivateKey(
                c.auth.private_key.clone().unwrap_or(
                    home::home_dir()
                        .map(|mut x| {
                            x.push(std::path::Path::new(".ssh/id_rsa"));
                            x.display().to_string()
                        })
                        .unwrap(),
                ),
            ),
            t_config::ConsoleSSHAuthType::Password => {
                t_console::SSHAuthAuth::Password(c.auth.password.clone().unwrap())
            }
        };
        let ssh_client = t_console::SSHClient::connect(
            c.timeout,
            &auth,
            c.username.clone(),
            format!("{}:{}", c.host, c.port),
        )
        .unwrap_or_else(|_| panic!("init ssh connection failed: {:?}", auth));
        info!(msg = "init ssh done");
        ssh_client
    }

    pub fn tty(&self) -> String {
        self.inner.tty.clone()
    }

    pub fn history(&mut self) -> String {
        self.history.push_str(self.inner.history().as_str());
        self.history.clone()
    }

    // TODO: may blocking
    pub fn exec_seperate(&mut self, command: &str) -> Result<(i32, String), io::Error> {
        use std::io::Read;
        let mut exec_ch = self.inner.session.channel_session().unwrap();

        exec_ch.exec(command)?;
        let mut buffer = String::new();
        exec_ch.read_to_string(&mut buffer)?;

        exec_ch.exec("echo $?\n")?;
        let mut code = String::new();
        exec_ch.read_to_string(&mut code)?;

        Ok((code.parse::<i32>().unwrap(), buffer))
    }

    pub fn write_string(&mut self, s: &str) -> Result<(), InteractError> {
        sleep(Duration::from_millis(100));
        self.inner
            .ctl
            .write_string(s)
            .map_err(|_| InteractError::Timeout)?;
        Ok(())
    }

    pub fn wait_string_ntimes(
        &mut self,
        timeout: Duration,
        pattern: &str,
        repeat: usize,
    ) -> Result<String, InteractError> {
        self.inner
            .ctl
            .wait_string_ntimes(timeout, pattern, repeat)
            .map_err(|_| InteractError::Timeout)
    }

    pub fn exec_global(
        &mut self,
        timeout: Duration,
        cmd: &str,
    ) -> Result<(i32, String), InteractError> {
        // "echo {}\n", \n may lost if no sleep
        sleep(Duration::from_millis(100));
        self.inner
            .ctl
            .exec_global(timeout, cmd)
            .map_err(|_| InteractError::Timeout)
    }

    pub fn upload_file(&mut self, remote_path: impl AsRef<Path>) {
        let p: &Path = remote_path.as_ref();
        assert!(p.exists());
        let stat = std::fs::metadata(p).unwrap();
        self.inner
            .session
            .scp_send(p, 644, stat.len(), None)
            .unwrap();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{env, thread};

    fn get_config_from_file() -> Option<t_config::Config> {
        let f = env::var("AUTOTEST_CONFIG_FILE").ok()?;
        t_config::load_config_from_file(f).map(Some).unwrap()
    }

    fn get_ssh_client() -> Option<SSHClient> {
        if let Some(c) = get_config_from_file() {
            return Some(SSHClient::new(c.console.ssh));
        }
        None
    }

    #[test]
    fn test_exec() {
        let cmds = vec![
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=\n"),
            (r#"export A=1;echo "A=$A""#, "A=1\n"),
        ];
        let ssh = get_ssh_client();
        if ssh.is_none() {
            return;
        }
        let mut ssh = ssh.unwrap();
        for cmd in cmds {
            let res = ssh.exec_seperate(cmd.0).unwrap();
            assert_eq!(res.1, cmd.1);
        }
    }

    #[test]
    fn test_tty_and_read_until() {
        let ssh = get_ssh_client();
        let ssh2 = get_ssh_client();
        if ssh.is_none() || ssh2.is_none() {
            return;
        }
        let mut ssh = ssh.unwrap();
        let mut ssh2 = ssh2.unwrap();

        let tty = ssh.tty();

        thread::spawn(move || {
            ssh2.exec_seperate(format!(r#"sleep 5 && echo "asdfg" > {}"#, tty).as_str())
        });

        ssh.wait_string_ntimes(Duration::from_secs(1), "asdfg", 1)
            .unwrap();
    }

    #[test]
    fn test_wr() {
        let ssh = get_ssh_client();
        if ssh.is_none() {
            return;
        }
        let mut ssh = ssh.unwrap();

        let cmds = vec![
            // (r#"echo "A=$A"\n"#, "A=\n"),
            ("touch ~/aaaaa", ""),
            ("echo \"111\"", "111\n"),
            ("export A=1", ""),
            ("echo A=$A", "A=1\n"),
            ("export A=2;echo A=$A", "A=2\n"),
        ];
        for cmd in cmds {
            let res = ssh.exec_global(Duration::from_secs(1), cmd.0).unwrap();
            assert_eq!(res.1, cmd.1);
        }
    }
}
