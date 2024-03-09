use crate::base::evloop::EvLoopCtl;
use crate::base::evloop::Req;
use crate::base::tty::Tty;
use crate::term::Term;
use crate::ConsoleError;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use tracing::error;
use tracing::{debug, info};

type Result<T> = std::result::Result<T, ConsoleError>;

#[derive(Debug)]
pub enum SSHAuthAuth<P: AsRef<Path>> {
    PrivateKey(P),
    Password(String),
}

pub struct SSH {
    c: t_config::ConsoleSSH,
    inner: SSHClient<crate::Xterm>,
    history: String,
}

impl SSH {
    pub fn new(c: t_config::ConsoleSSH) -> Result<Self> {
        let mut inner = Self::connect_from_ssh_config(&c)?;

        debug!(msg = "ssh getting tty...");
        let (code, tty) = inner
            .pts
            .exec_global(Duration::from_secs(10), "tty")
            .map_err(|e| ConsoleError::ConnectionBroken(format!("ssh get tty failed, {}", e)))?;

        if code != 0 {
            return Err(ConsoleError::ConnectionBroken(
                "run tty command failed".to_string(),
            ));
        }

        inner.pts_file = tty;
        info!(msg = "ssh client tty", tty = inner.pts_file.trim());

        Ok(Self {
            c,
            inner,
            history: "".to_string(),
        })
    }

    pub fn stop(&self) {
        if let Err(_) = self.inner.pts.send(Req::Stop) {
            error!("ssh evloop stopped failed");
        }
    }

    pub fn reconnect(&mut self) -> Result<()> {
        self.history.push_str(self.inner.history().as_str());
        self.inner = Self::connect_from_ssh_config(&self.c)?;
        Ok(())
    }

    fn connect_from_ssh_config(c: &t_config::ConsoleSSH) -> Result<SSHClient<crate::Xterm>> {
        info!(msg = "init ssh...");
        let auth = match c.auth.r#type {
            t_config::ConsoleSSHAuthType::PrivateKey => SSHAuthAuth::PrivateKey(
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
                SSHAuthAuth::Password(c.auth.password.clone().unwrap())
            }
        };
        SSHClient::connect(
            c.timeout,
            &auth,
            c.username.clone(),
            format!("{}:{}", c.host, c.port),
        )
        .map_err(|e| ConsoleError::ConnectionBroken(e.to_string()))
    }

    pub fn tty(&self) -> String {
        self.inner.pts_file.clone()
    }

    pub fn history(&mut self) -> String {
        self.history.push_str(self.inner.history().as_str());
        self.history.clone()
    }

    // TODO: may blocking
    pub fn exec_seperate(
        &mut self,
        command: &str,
    ) -> std::result::Result<(i32, String), std::io::Error> {
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

    fn do_with_reconnect<T>(&mut self, f: impl Fn(&mut Self) -> Result<T>) -> Result<T> {
        let mut retry = 3;
        loop {
            retry -= 1;
            if retry == 0 {
                return Err(ConsoleError::ConnectionBroken(
                    "reconnect exceed max time".to_string(),
                ));
            }

            match f(self) {
                Ok(v) => return Ok(v),
                Err(e) => match e {
                    ConsoleError::ConnectionBroken(_) => {
                        let _ = self.reconnect();
                        continue;
                    }
                    _ => return Err(ConsoleError::Timeout),
                },
            }
        }
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        sleep(Duration::from_millis(100));
        self.do_with_reconnect(|c| c.inner.pts.write_string(s))?;
        Ok(())
    }

    pub fn exec_global(&mut self, timeout: Duration, cmd: &str) -> Result<(i32, String)> {
        // "echo {}\n", \n may lost if no sleep
        sleep(Duration::from_millis(100));
        self.do_with_reconnect(|c| c.inner.pts.exec_global(timeout, cmd))
    }

    pub fn wait_string_ntimes(
        &mut self,
        timeout: Duration,
        pattern: &str,
        repeat: usize,
    ) -> Result<String> {
        self.do_with_reconnect(|c| c.inner.pts.wait_string_ntimes(timeout, pattern, repeat))
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

struct SSHClient<T: Term> {
    session: ssh2::Session,
    pts: Tty<T>,
    pts_file: String,
}

impl<Tm> SSHClient<Tm>
where
    Tm: Term,
{
    pub fn connect<P: AsRef<Path>, A: ToSocketAddrs>(
        timeout: Option<Duration>,
        auth: &SSHAuthAuth<P>,
        user: impl Into<String>,
        addrs: A,
    ) -> std::result::Result<Self, std::io::Error> {
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

        let res = Self {
            session: sess,
            pts: Tty::new(EvLoopCtl::new(channel)),
            pts_file: "".to_string(),
        };

        Ok(res)
    }

    pub fn history(&self) -> String {
        Tm::parse_and_strip(&self.pts.history())
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

    fn get_ssh_client() -> Option<SSH> {
        if let Some(c) = get_config_from_file() {
            return SSH::new(c.console.ssh).ok();
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
