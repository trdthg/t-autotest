use super::text_console::BufEvLoopCtl;
use crate::term::Term;
use crate::EvLoopCtl;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use tracing::debug;

pub struct SSHClient<T: Term> {
    pub session: ssh2::Session,
    pub ctl: BufEvLoopCtl<T>,
    pub tty: String,
}

#[derive(Debug)]
pub enum SSHAuthAuth<P: AsRef<Path>> {
    PrivateKey(P),
    Password(String),
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
    ) -> Result<Self, std::io::Error> {
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
            ctl: BufEvLoopCtl::new(EvLoopCtl::new(channel)),
            tty: "".to_string(),
        };

        Ok(res)
    }

    pub fn history(&self) -> String {
        Tm::parse_and_strip(&self.ctl.history())
    }
}
