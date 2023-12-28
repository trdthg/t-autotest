use anyhow::Result;
use ssh2::DisconnectCode;
use std::io::{self, BufReader};
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

    pub fn call(&mut self, command: &str) -> Result<u32> {
        let mut channel = self.session.channel_session()?;
        channel.exec(command)?;
        let mut stdout = std::io::stdout();
        io::copy(&mut BufReader::new(channel), &mut stdout)?;
        Ok(0)
    }

    pub fn disconnect(&mut self) -> Result<()> {
        self.session
            .disconnect(Some(DisconnectCode::ByApplication), "user close", None)?;
        Ok(())
    }
}
