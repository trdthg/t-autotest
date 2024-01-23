use super::text_console::BufEvLoopCtl;
use crate::{parse_str_from_vt100_bytes, EvLoopCtl};
use anyhow::Result;
use std::fs;
use std::io::Read;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use tracing::{debug, info};

/// This struct is a convenience wrapper
/// around a russh client
pub struct SSHClient {
    session: ssh2::Session,
    ctl: BufEvLoopCtl,
    tty: String,
}

#[derive(Debug)]
pub enum SSHAuthAuth<P: AsRef<Path>> {
    PrivateKey(P),
    Password(String),
}

impl SSHClient {
    pub fn connect<P: AsRef<Path>, A: ToSocketAddrs>(
        timeout: Option<Duration>,
        auth: SSHAuthAuth<P>,
        user: impl Into<String>,
        addrs: A,
    ) -> Result<Self> {
        let tcp = TcpStream::connect(addrs)?;
        let mut sess = ssh2::Session::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;

        // never disconnect auto
        sess.set_timeout(timeout.map(|x| x.as_millis() as u32).unwrap_or(5000));

        match auth {
            SSHAuthAuth::PrivateKey(private_key) => {
                sess.userauth_pubkey_file(&user.into(), None, private_key.as_ref(), None)?;
            }
            SSHAuthAuth::Password(password) => {
                sess.userauth_password(&user.into(), password.as_str())?;
            }
        }
        assert!(sess.authenticated());
        debug!(msg = "ssh auth success");

        // build shell channel
        let mut channel = sess.channel_session()?;
        channel
            .request_pty("xterm", None, Some((80, 24, 0, 0)))
            .unwrap();
        channel.shell().unwrap();

        sleep(Duration::from_secs(3));

        let mut res = Self {
            session: sess,
            ctl: BufEvLoopCtl::new(EvLoopCtl::new(channel)),
            tty: "".to_string(),
        };

        debug!(msg = "ssh getting tty...");
        let (ok, tty) = res.exec_global(Duration::from_secs(10), "tty").unwrap();
        if ok != 0 {
            return Err(anyhow::anyhow!("get tty failed"));
        }
        res.tty = tty;
        info!(msg = "ssh client tty", tty = res.tty.trim());

        Ok(res)
    }

    pub fn tty(&self) -> String {
        self.tty.clone()
    }

    pub fn dump_history(&self) -> String {
        parse_str_from_vt100_bytes(&self.ctl.history())
    }

    // TODO: may blocking
    pub fn exec_seperate(&mut self, command: &str) -> Result<(i32, String)> {
        let mut exec_ch = self.session.channel_session().unwrap();

        exec_ch.exec(command)?;
        let mut buffer = String::new();
        exec_ch.read_to_string(&mut buffer)?;

        exec_ch.exec("echo $?\n")?;
        let mut code = String::new();
        exec_ch.read_to_string(&mut code)?;

        Ok((code.parse::<i32>()?, buffer))
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        sleep(Duration::from_millis(100));
        self.ctl.write_string(s)?;
        Ok(())
    }

    pub fn read_golbal_until(&mut self, timeout: Duration, pattern: &str) -> Result<()> {
        self.ctl
            .comsume_buffer_and_map(timeout, |buffer| {
                let buffer_str = parse_str_from_vt100_bytes(buffer);
                buffer_str.find(pattern)
            })
            .map(|_| ())
    }

    pub fn exec_global(&mut self, timeout: Duration, cmd: &str) -> Result<(i32, String)> {
        // "echo {}\n", \n may lost if no sleep
        sleep(Duration::from_millis(100));
        self.ctl.exec_global(timeout, cmd)
    }

    pub fn upload_file(&mut self, remote_path: impl AsRef<Path>) {
        let p: &Path = remote_path.as_ref();
        assert!(p.exists());
        let stat = fs::metadata(p).unwrap();
        self.session.scp_send(p, 644, stat.len(), None).unwrap();
    }
}

#[cfg(test)]
mod test {
    use std::{env, thread};

    use super::*;

    fn get_config_from_file() -> Option<t_config::Config> {
        let f = env::var("AUTOTEST_CONFIG_FILE").ok()?;
        t_config::load_config_from_file(f).map(Some).unwrap()
    }

    fn get_ssh_client() -> Option<SSHClient> {
        let c = get_config_from_file()?;
        assert!(c.console.ssh.enable);

        let key_path = c.console.ssh.auth.private_key;
        let username = c.console.ssh.username;
        let addrs = format!("{}:{}", c.console.ssh.host, c.console.ssh.port);

        let auth = match c.console.ssh.auth.r#type {
            t_config::ConsoleSSHAuthType::PrivateKey => SSHAuthAuth::PrivateKey(
                key_path.clone().unwrap_or(
                    home::home_dir()
                        .map(|mut p| {
                            p.push(".ssh/id_rsa");
                            p.to_str().unwrap().to_string()
                        })
                        .unwrap(),
                ),
            ),
            t_config::ConsoleSSHAuthType::Password => {
                SSHAuthAuth::Password(c.console.ssh.auth.password.unwrap())
            }
        };

        dbg!(&key_path, &username, &addrs);
        let serial = SSHClient::connect(None, auth, username, addrs).unwrap();
        Some(serial)
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

        ssh.read_golbal_until(Duration::from_secs(1), "asdfg")
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
