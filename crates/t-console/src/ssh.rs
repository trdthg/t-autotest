use anyhow::Result;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use tracing::{debug, info, trace};

use crate::get_parsed_str_from_xt100_bytes;

use super::DuplexChannelConsole;

/// This struct is a convenience wrapper
/// around a russh client
pub struct SSHClient {
    session: ssh2::Session,
    shell: ssh2::Channel,
    tty: String,
    buffer: Vec<u8>,
    history: Vec<u8>,
}

impl DuplexChannelConsole for SSHClient {}

#[derive(Debug)]
pub enum SSHAuthAuth<P: AsRef<Path>> {
    PrivateKey(P),
    Password(String),
}

impl SSHClient {
    pub fn connect<P: AsRef<Path>, A: ToSocketAddrs>(
        auth: SSHAuthAuth<P>,
        user: impl Into<String>,
        addrs: A,
    ) -> Result<Self> {
        let tcp = TcpStream::connect(addrs)?;
        let mut sess = ssh2::Session::new()?;
        sess.set_tcp_stream(tcp);
        sess.handshake()?;

        // never disconnect auto
        sess.set_timeout(0);

        match auth {
            SSHAuthAuth::PrivateKey(private_key) => {
                sess.userauth_pubkey_file(&user.into(), None, private_key.as_ref(), None)?;
            }
            SSHAuthAuth::Password(password) => {
                sess.userauth_password(&user.into(), password.as_str())?;
            }
        }
        assert!(sess.authenticated());
        debug!("ssh auth success");

        let mut channel = sess.channel_session()?;
        channel
            .request_pty("xterm", None, Some((80, 24, 0, 0)))
            .unwrap();
        channel.shell().unwrap();

        sleep(Duration::from_secs(3));

        let mut res = Self {
            session: sess,
            shell: channel,
            tty: "".to_string(),
            buffer: Vec::new(),
            history: Vec::new(),
        };

        debug!("ssh getting tty...");
        let tty = res.exec_global("tty").unwrap();
        res.tty = tty;
        info!("ssh client tty: [{}]", res.tty.trim());

        Ok(res)
    }

    pub fn tty(&self) -> String {
        return self.tty.clone();
    }

    pub fn history(&self) -> String {
        return get_parsed_str_from_xt100_bytes(&self.history.clone());
    }

    pub fn exec_seperate(&mut self, command: &str) -> Result<String> {
        let mut exec_ch = self.session.channel_session().unwrap();

        exec_ch.exec(command)?;
        let mut buffer = String::new();
        exec_ch.read_to_string(&mut buffer)?;
        Ok(buffer)
    }

    pub fn write_global(&mut self, command: &str) -> Result<()> {
        sleep(Duration::from_millis(100));
        let ch = &mut self.shell;
        ch.write_all(command.as_bytes()).unwrap();
        ch.flush().unwrap();
        Ok(())
    }

    pub fn read_golbal_until(&mut self, pattern: &str) -> Result<()> {
        let ch = &mut self.shell;

        let current_buffer_start = self.buffer.len();

        loop {
            let mut output_buffer = [0u8; 1024];
            match ch.read(&mut output_buffer) {
                Ok(n) => {
                    let received = &output_buffer[0..n];

                    // save to buffer
                    self.buffer.extend(received);
                    self.history.extend(received);

                    // find target pattern
                    let buffer_str = get_parsed_str_from_xt100_bytes(&self.buffer);
                    if buffer_str.find(pattern).is_none() {
                        continue;
                    }

                    // cut from last find
                    self.buffer = self.buffer[current_buffer_start..].to_owned();
                    return Ok(());
                }
                Err(_) => unreachable!(),
            }
        }
    }

    pub fn exec_global(&mut self, command: &str) -> Result<String> {
        // "echo {}\n", \n may lost if no sleep
        sleep(Duration::from_millis(100));

        let ch = &mut self.shell;
        // write user command
        ch.write_all(command.as_bytes()).unwrap();

        // write nanoid for regex
        let nanoid = nanoid::nanoid!();
        ch.write_all(format!("; echo {}\n", nanoid).as_bytes())
            .unwrap();

        ch.flush().unwrap();

        let current_buffer_start = self.buffer.len();

        loop {
            let mut output_buffer = [0u8; 1024];
            match ch.read(&mut output_buffer) {
                Ok(n) => {
                    let received = &output_buffer[0..n];

                    // save to buffer
                    self.buffer.extend(received);
                    self.history.extend(received);

                    // find target pattern from buffer
                    let parsed_str = get_parsed_str_from_xt100_bytes(&self.buffer);
                    trace!("current buffer: [{:?}]", get_parsed_str_from_xt100_bytes(&self.buffer));

                    let res = t_util::assert_capture_between(
                        &parsed_str,
                        &format!("{nanoid}\n"),
                        &nanoid,
                    )
                    .unwrap();
                    if res.is_none() {
                        continue;
                    }

                    self.buffer = self.buffer[current_buffer_start..].to_owned();

                    return Ok(res.unwrap());
                }
                Err(_) => unreachable!(),
            }
        }
    }

    pub fn upload_file(&mut self, remote_path: impl AsRef<Path>) {
        let p: &Path = remote_path.as_ref();
        assert!(p.exists());
        let stat = fs::metadata(p).unwrap();
        self.session.scp_send(p, 0644, stat.len(), None).unwrap();
    }
}

#[cfg(test)]
mod test {
    use std::{env, thread};

    use super::*;

    #[test]
    fn test_exec() {
        let key_path = env::var("KEY_PATH").unwrap_or("~/.ssh/id_rsa".to_string());
        let username = env::var("USERNAME").unwrap_or("root".to_string());
        let addrs = env::var("ADDRS").unwrap_or("127.0.0.1:22".to_string());

        dbg!(&key_path, &username, &addrs);
        let mut serial =
            SSHClient::connect(SSHAuthAuth::PrivateKey(key_path), username, addrs).unwrap();

        let cmds = vec![
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=\n"),
            (r#"export A=1;echo "A=$A""#, "A=1\n"),
        ];
        for cmd in cmds {
            let res = serial.exec_seperate(cmd.0).unwrap();
            assert_eq!(res, cmd.1);
        }
    }

    #[test]
    fn test_tty_and_read_until() {
        let key_path = env::var("KEY_PATH").unwrap_or("~/.ssh/id_rsa".to_string());
        let username = env::var("USERNAME").unwrap_or("root".to_string());
        let addrs = env::var("ADDRS").unwrap_or("127.0.0.1:22".to_string());

        dbg!(&key_path, &username, &addrs);
        let mut ssh =
            SSHClient::connect(SSHAuthAuth::PrivateKey(&key_path), &username, &addrs).unwrap();
        let mut ssh2 =
            SSHClient::connect(SSHAuthAuth::PrivateKey(key_path), &username, &addrs).unwrap();

        let tty = ssh.tty();

        thread::spawn(move || {
            ssh2.exec_seperate(format!(r#"sleep 5 && echo "asdfg" > {}"#, tty).as_str())
        });

        ssh.read_golbal_until("asdfg").unwrap();
    }

    #[test]
    fn test_wr() {
        let key_path = env::var("KEY_PATH").unwrap_or("~/.ssh/id_rsa".to_string());
        let username = env::var("USERNAME").unwrap_or("root".to_string());
        let addrs = env::var("ADDRS").unwrap_or("127.0.0.1:22".to_string());

        dbg!(&key_path, &username, &addrs);
        let mut sshc =
            SSHClient::connect(SSHAuthAuth::PrivateKey(key_path), username, addrs).unwrap();

        let cmds = vec![
            // (r#"echo "A=$A"\n"#, "A=\n"),
            ("touch ~/aaaaa", ""),
            ("echo \"111\"", "111\n"),
            ("export A=1", ""),
            ("echo A=$A", "A=1\n"),
            ("export A=2;echo A=$A", "A=2\n"),
        ];
        for cmd in cmds {
            let res = sshc.exec_global(cmd.0).unwrap();
            assert_eq!(res, cmd.1);
        }
    }
}
