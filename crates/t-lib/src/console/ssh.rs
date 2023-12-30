use anyhow::Result;
use ssh2::{Channel, DisconnectCode};
use std::io::{self, BufReader, Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;

use super::DuplexChannelConsole;

/// This struct is a convenience wrapper
/// around a russh client
pub struct SSHClient {
    session: ssh2::Session,
}

impl DuplexChannelConsole for SSHClient {}

impl SSHClient {
    pub fn connect<P: AsRef<Path>, A: ToSocketAddrs>(
        key_path: P,
        user: impl Into<String>,
        addrs: A,
    ) -> Result<Self> {
        let tcp = TcpStream::connect(addrs)?;
        let mut sess = ssh2::Session::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;

        // never disconnect auto
        sess.set_timeout(0);

        // Try to authenticate with the first identity in the agent.
        sess.userauth_pubkey_file(&user.into(), None, key_path.as_ref(), None)?;

        // Make sure we succeeded

        assert!(sess.authenticated());
        Ok(Self { session: sess })
    }

    pub fn exec(&mut self, command: &str) -> Result<String> {
        let mut ch = self.session.channel_session()?;

        ch.exec(command)?;
        let mut s = String::new();
        ch.read_to_string(&mut s).unwrap();
        Ok(s)
    }

    pub fn wr(&mut self, command: &str) -> Result<String> {
        let ch = self.session.channel_session()?;
        dbg!();

        let mut ch = ch.stream(1);
        dbg!();

        ch.flush()?;
        dbg!();

        ch.write_all(command.as_bytes()).unwrap();
        dbg!();

        let mut s = String::new();
        ch.read_to_string(&mut s).unwrap();
        dbg!();

        Ok(s)
    }

    pub fn disconnect(&mut self) -> Result<()> {
        self.session
            .disconnect(Some(DisconnectCode::ByApplication), "user close", None)?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::env;

    use super::*;

    #[test]
    fn test_exec() {
        let key_path = env::var("KEY_PATH").unwrap_or("~/.ssh/id_rsa".to_string());
        let username = env::var("USERNAME").unwrap_or("root".to_string());
        let addrs = env::var("ADDRS").unwrap_or("127.0.0.1:22".to_string());

        dbg!(&key_path, &username, &addrs);
        let mut serial = SSHClient::connect(key_path, username, addrs).unwrap();

        let cmds = vec![
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=\n"),
            (r#"export A=1;echo "A=$A""#, "A=1\n"),
        ];
        for cmd in cmds {
            let res = serial.exec(cmd.0).unwrap();
            assert_eq!(res, cmd.1);
        }
    }

    #[test]
    fn test_wr() {
        let key_path = env::var("KEY_PATH").unwrap_or("~/.ssh/id_rsa".to_string());
        let username = env::var("USERNAME").unwrap_or("root".to_string());
        let addrs = env::var("ADDRS").unwrap_or("127.0.0.1:22".to_string());

        dbg!(&key_path, &username, &addrs);
        let mut sshc = SSHClient::connect(key_path, username, addrs).unwrap();

        let cmds = vec![
            ("export A=1\n", ""),
            // (r#"echo "A=$A"\n"#, "A=\n"),
            // (r#"export A=1;echo "A=$A"\n"#, "A=1\n"),
        ];
        for cmd in cmds {
            let res = sshc.wr(cmd.0).unwrap();
            assert_eq!(res, cmd.1);
        }
    }
}
