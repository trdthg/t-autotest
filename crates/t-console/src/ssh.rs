use crate::base::evloop::EventLoop;
use crate::base::tty::Tty;
use crate::term::Term;
use crate::ConsoleError;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::ops::Deref;
use std::ops::DerefMut;
use std::path::Path;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use tracing::{debug, info};

type Result<T> = std::result::Result<T, ConsoleError>;

#[derive(Debug)]
pub enum SSHAuthAuth<P: AsRef<Path>> {
    PrivateKey(P),
    Password(String),
}

pub struct SSH {
    inner: SSHClient<crate::Xterm>,
}

impl Deref for SSH {
    type Target = Tty<crate::Xterm>;

    fn deref(&self) -> &Self::Target {
        &self.inner.pts
    }
}

impl DerefMut for SSH {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner.pts
    }
}

impl SSH {
    pub fn new(c: t_config::ConsoleSSH) -> Result<Self> {
        let inner = Self::connect_from_ssh_config(&c)?;

        // debug!(msg = "ssh getting tty...");
        // let (code, tty) = inner.pts.exec(Duration::from_secs(10), "tty")?;

        // if code != 0 {
        //     return Err(ConsoleError::NoBashSupport(format!(
        //         "run tty command failed, code: {}, tty: {}",
        //         code, tty
        //     )));
        // }

        // inner.pts_file = tty;
        // info!(msg = "ssh client tty", tty = inner.pts_file.trim());

        Ok(Self { inner })
    }

    fn connect_from_ssh_config(c: &t_config::ConsoleSSH) -> Result<SSHClient<crate::Xterm>> {
        info!(msg = "init ssh...");
        let auth = if let Some(password) = c.password.as_ref() {
            SSHAuthAuth::Password(password.clone())
        } else {
            SSHAuthAuth::PrivateKey(
                c.private_key.clone().unwrap_or(
                    home::home_dir()
                        .map(|mut x| {
                            x.push(std::path::Path::new(".ssh/id_rsa"));
                            x.display().to_string()
                        })
                        .unwrap(),
                ),
            )
        };
        SSHClient::connect(
            c.timeout,
            &auth,
            c.username.clone(),
            format!("{}:{}", c.host, c.port.unwrap_or(22)),
            c.log_file.clone(),
        )
    }

    pub fn tty(&self) -> String {
        self.inner.pts_file.clone()
    }

    // FIXME: may blocking
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
        log_file: Option<PathBuf>,
    ) -> std::result::Result<Self, ConsoleError> {
        let tcp = TcpStream::connect(addrs).map_err(ConsoleError::IO)?;
        let mut sess = ssh2::Session::new().map_err(ConsoleError::SSH2)?;
        sess.set_tcp_stream(tcp);
        sess.handshake().map_err(ConsoleError::SSH2)?;

        // never disconnect auto
        sess.set_timeout(timeout.map(|x| x.as_millis() as u32).unwrap_or(5000));

        match auth {
            SSHAuthAuth::PrivateKey(private_key) => {
                sess.userauth_pubkey_file(&user.into(), None, private_key.as_ref(), None)
                    .map_err(ConsoleError::SSH2)?;
            }
            SSHAuthAuth::Password(password) => {
                sess.userauth_password(&user.into(), password.as_str())
                    .map_err(ConsoleError::SSH2)?;
            }
        }
        assert!(sess.authenticated());
        debug!(msg = "ssh auth success");

        sleep(Duration::from_secs(3));

        let res = Self {
            session: sess.clone(),
            pts: Tty::new(EventLoop::spawn(
                move || {
                    // build shell channel
                    let mut channel = sess.channel_session().map_err(ConsoleError::SSH2)?;
                    channel
                        .request_pty("xterm", None, Some((80, 24, 0, 0)))
                        .map_err(ConsoleError::SSH2)?;
                    channel.shell().map_err(ConsoleError::SSH2)?;
                    Ok(channel)
                },
                log_file,
            )?),
            pts_file: "".to_string(),
        };

        Ok(res)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{env, thread};

    fn get_config_from_file() -> Option<t_config::Config> {
        let f = env::var("AUTOTEST_CONFIG_FILE").ok()?;
        t_config::load_config_from_file(f).ok()
    }

    fn get_ssh_client() -> Option<SSH> {
        if let Some(c) = get_config_from_file() {
            return SSH::new(c.ssh?).ok();
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
            let res = ssh.exec(Duration::from_secs(1), cmd.0).unwrap();
            assert_eq!(res.1, cmd.1);
        }
    }
}
