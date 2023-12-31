use anyhow::Result;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use crate::get_parsed_str_from_xt100_bytes;

use super::DuplexChannelConsole;

/// This struct is a convenience wrapper
/// around a russh client
pub struct SSHClient {
    session: ssh2::Session,
    shell: ssh2::Channel,
    buffer: String,
    history: String,
}

impl DuplexChannelConsole for SSHClient {
    fn exec(&mut self, cmd: &str) -> String {
        self.exec(cmd).unwrap()
    }
}

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
        assert!(sess.authenticated());

        let mut channel = sess.channel_session()?;
        channel
            .request_pty("xterm", None, Some((80, 24, 0, 0)))
            .unwrap();
        channel.shell().unwrap();

        sleep(Duration::from_secs(3));

        Ok(Self {
            session: sess,
            shell: channel,
            buffer: String::new(),
            history: String::new(),
        })
    }

    pub fn exec(&mut self, command: &str) -> Result<String> {
        let mut exec_ch = self.session.channel_session().unwrap();

        exec_ch.exec(command)?;
        let mut buffer = String::new();
        exec_ch.read_to_string(&mut buffer)?;
        Ok(buffer)
    }

    pub fn wr(&mut self, command: &str) -> Result<String> {
        // "echo {}\n", \n may lost if no sleep
        sleep(Duration::from_millis(100));

        let ch = &mut self.shell;
        // write user command
        ch.write(command.as_bytes()).unwrap();

        // write nanoid for regex
        let nanoid = nanoid::nanoid!();
        ch.write_all(format!("; echo {}\n", nanoid).as_bytes())
            .unwrap();

        ch.flush().unwrap();

        loop {
            let mut output_buffer = [0u8; 1024];
            match ch.read(&mut output_buffer) {
                Ok(n) => {
                    let received = &output_buffer[0..n];

                    let parsed_str = get_parsed_str_from_xt100_bytes(received);
                    self.buffer.push_str(&parsed_str);
                    self.history.push_str(&parsed_str);

                    let buffered_output = &mut self.buffer;

                    let res = t_util::assert_capture_between(
                        &buffered_output,
                        &format!("{nanoid}\n"),
                        &nanoid,
                    )
                    .unwrap();
                    if res.is_none() {
                        continue;
                    }

                    let first_place = buffered_output.find(nanoid.as_str()).unwrap();
                    let update = buffered_output[first_place + nanoid.len() + 1..].to_owned();
                    self.buffer = update;

                    return Ok(res.unwrap());
                }
                Err(_) => unreachable!(),
            }
        }
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
            // (r#"echo "A=$A"\n"#, "A=\n"),
            ("touch ~/aaaaa", ""),
            ("echo \"111\"", "111\n"),
            ("export A=1", ""),
            ("echo A=$A", "A=1\n"),
            ("export A=2;echo A=$A", "A=2\n"),
        ];
        for cmd in cmds {
            let res = sshc.wr(cmd.0).unwrap();
            assert_eq!(res, cmd.1);
        }
    }
}
