use super::DuplexChannelConsole;
use crate::parse_str_from_vt100_bytes;
use anyhow::Result;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, UNIX_EPOCH};
use std::{fs, time};
use tracing::{debug, info, trace};

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
        debug!(msg = "ssh auth success");

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

        debug!(msg = "ssh getting tty...");
        let tty = res.exec_global(Duration::from_secs(10), "tty").unwrap();
        res.tty = tty;
        info!(msg = "ssh client tty", tty = res.tty.trim());

        Ok(res)
    }

    pub fn tty(&self) -> String {
        return self.tty.clone();
    }

    pub fn dump_history(&self) -> String {
        return parse_str_from_vt100_bytes(&self.history.clone());
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

    fn comsume_buffer_and_map<T>(
        &mut self,
        timeout: Duration,
        f: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<T> {
        let ch = &mut self.shell;

        let current_buffer_start = self.buffer.len();

        let start = time::SystemTime::now();

        loop {
            let mut output_buffer = [0u8; 1024];
            if time::SystemTime::now().duration_since(start).unwrap() > timeout {
                break;
            }
            match ch.read(&mut output_buffer) {
                Ok(n) => {
                    let received = &output_buffer[0..n];

                    // save to buffer
                    self.buffer.extend(received);
                    self.history.extend(received);

                    // find target pattern
                    let res = f(&self.buffer);

                    // if buffer_str.find(pattern).is_none() {
                    if res.is_none() {
                        continue;
                    }

                    // cut from last find
                    self.buffer = self.buffer[current_buffer_start..].to_owned();
                    return Ok(res.unwrap());
                }
                Err(_) => unreachable!(),
            }
        }
        return Err(anyhow::anyhow!("timeout"));
    }

    pub fn read_golbal_until(&mut self, timeout: Duration, pattern: &str) -> Result<()> {
        self.comsume_buffer_and_map(timeout, |buffer| {
            let buffer_str = parse_str_from_vt100_bytes(buffer);
            buffer_str.find(pattern)
        })
        .map(|_| ())
    }

    pub fn exec_global(&mut self, timeout: Duration, command: &str) -> Result<String> {
        // "echo {}\n", \n may lost if no sleep
        sleep(Duration::from_millis(100));

        let ch = &mut self.shell;

        // write nanoid for regex
        let nanoid = nanoid::nanoid!();
        let cmd = format!("{command}; echo {}\n", nanoid);
        ch.write_all(cmd.as_bytes()).unwrap();
        ch.flush().unwrap();

        self.comsume_buffer_and_map(timeout, |buffer| {
            // find target pattern from buffer
            let parsed_str = parse_str_from_vt100_bytes(buffer);
            debug!("current buffer: [{:?}]", parsed_str);
            let res = t_util::assert_capture_between(&parsed_str, &format!("{nanoid}\n"), &nanoid)
                .unwrap();
            res
        })
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

    fn get_config_from_file() -> t_config::Config {
        let f = env::var("AUTOTEST_CONFIG_FILE").unwrap();
        t_config::load_config_from_file(f).unwrap()
    }

    fn get_ssh_client() -> SSHClient {
        let c = get_config_from_file();
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
        let serial = SSHClient::connect(auth, username, addrs).unwrap();
        serial
    }

    #[test]
    fn test_exec() {
        let cmds = vec![
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=\n"),
            (r#"export A=1;echo "A=$A""#, "A=1\n"),
        ];
        let mut ssh = get_ssh_client();
        for cmd in cmds {
            let res = ssh.exec_seperate(cmd.0).unwrap();
            assert_eq!(res, cmd.1);
        }
    }

    #[test]
    fn test_tty_and_read_until() {
        let mut ssh = get_ssh_client();
        let mut ssh2 = get_ssh_client();

        let tty = ssh.tty();

        thread::spawn(move || {
            ssh2.exec_seperate(format!(r#"sleep 5 && echo "asdfg" > {}"#, tty).as_str())
        });

        ssh.read_golbal_until(Duration::from_secs(1), "asdfg")
            .unwrap();
    }

    #[test]
    fn test_wr() {
        let mut ssh = get_ssh_client();

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
            assert_eq!(res, cmd.1);
        }
    }
}
