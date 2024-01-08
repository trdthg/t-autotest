use super::DuplexChannelConsole;
use crate::parse_str_from_xt100_bytes;
use anyhow::Result;
use byteorder::WriteBytesExt;
use serialport::TTYPort;
use std::io::{self, Read, Write};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use t_util::ExecutorError;
use tracing::{debug, info, trace};

pub struct SerialClient {
    conn: TTYPort,
    buffer: Vec<u8>,
    history: Vec<u8>,
}

impl DuplexChannelConsole for SerialClient {}

#[derive(Debug)]
pub enum SerialError {
    ConnectError(String),
    Read(io::Error),
    Write(io::Error),
    STTY(ExecutorError),
}

impl Drop for SerialClient {
    fn drop(&mut self) {
        println!("dropped");
        self.conn.write_u8(0x04).unwrap();
        self.conn.flush().unwrap();
    }
}

impl SerialClient {
    pub fn dump_history(&self) -> String {
        parse_str_from_xt100_bytes(&self.history)
    }

    pub fn connect(
        file: impl Into<String>,
        bund_rate: u32,
        timeout: Duration,
        auth: Option<(impl Into<String>, impl Into<String>)>,
    ) -> Result<Self, SerialError> {
        let file: String = file.into();
        let path = Path::new(&file);
        if !path.exists() {
            panic!("serial path not exists");
        }

        // init tty
        t_util::execute_shell(
            format!("stty -F {} {} -echo -icrnl -onlcr -icanon", file, bund_rate).as_str(),
        )
        .map_err(|e| SerialError::STTY(e))?;

        let port = serialport::new(&file, bund_rate)
            .timeout(timeout)
            .open_native()
            .map_err(|e| SerialError::ConnectError(e.to_string()))?;

        let mut res = Self {
            conn: port,
            buffer: Vec::new(),
            history: Vec::new(),
        };

        if let Some((username, password)) = auth {
            res.login(&username.into(), &password.into());
        }

        Ok(res)
    }

    fn login(&mut self, username: &str, password: &str) {
        // logout
        self.conn.write_u8(0x04).unwrap();
        self.conn.write_all("\n".as_bytes()).unwrap();
        self.conn.flush().unwrap();
        sleep(Duration::from_millis(5000));

        // username
        self.conn
            .write_all(format!("{username}\n").as_bytes())
            .unwrap();
        self.conn.flush().unwrap();
        sleep(Duration::from_millis(5000));

        // password
        self.conn
            .write_all(format!("{password}\n").as_bytes())
            .unwrap();
        self.conn.flush().unwrap();
        sleep(Duration::from_millis(3000));

        info!("{}", "try login done");
    }

    pub fn exec_global(&mut self, cmd: &str) -> Result<String> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        sleep(Duration::from_millis(70));

        let nanoid = nanoid::nanoid!();
        let cmd = format!("{cmd}; echo {}\n", nanoid);
        self.conn.write_all(cmd.as_bytes()).unwrap();
        self.conn.flush().unwrap();

        self.comsume_buffer_and_map(|buffer| {
            // find target pattern from buffer
            let parsed_str = parse_str_from_xt100_bytes(buffer);
            let res = t_util::assert_capture_between(&parsed_str, &format!("{nanoid}\n"), &nanoid)
                .unwrap();
            trace!(nanoid = nanoid, parsed_str = parsed_str);
            res
        })
    }

    pub fn read_golbal_until(&mut self, pattern: &str) -> Result<()> {
        self.comsume_buffer_and_map(|buffer| {
            let buffer_str = parse_str_from_xt100_bytes(buffer);
            buffer_str.find(pattern)
        })
        .map(|_| ())
    }

    fn comsume_buffer_and_map<T>(&mut self, f: impl Fn(&[u8]) -> Option<T>) -> Result<T> {
        let conn = &mut self.conn;

        let current_buffer_start = self.buffer.len();

        loop {
            let mut output_buffer = [0u8; 1024];
            match conn.read(&mut output_buffer) {
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
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    continue;
                }
                Err(e) => {
                    panic!("{}", format!("{}", e));
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use t_config::Config;
    use tracing::{info, trace};

    use crate::SerialClient;
    use std::{env, time::Duration};

    fn get_config_from_file() -> Config {
        let f = env::var("AUTOTEST_CONFIG_FILE").unwrap();
        t_config::load_config_from_file(f).unwrap()
    }

    fn get_client(c: &Config) -> SerialClient {
        assert!(c.console.serial.enable);

        let c = c.console.serial.clone();

        let auth = if c.auto_login {
            Some((c.username.unwrap(), c.password.unwrap()))
        } else {
            None
        };

        let serial = SerialClient::connect(
            &c.serial_file,
            c.bund_rate,
            Duration::from_secs(10000),
            auth,
        )
        .unwrap();
        serial
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_exec_global() {
        let c = get_config_from_file();
        let mut serial = get_client(&c);

        let cmds = vec![
            ("unset A", ""),
            (r#"echo "A=$A""#, "A=\n"),
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=1\n"),
        ];

        (0..10).for_each(|_| {
            for cmd in cmds.iter() {
                trace!(cmd = cmd.0);
                let res = serial.exec_global(cmd.0).unwrap();
                assert_eq!(res, cmd.1);
            }
        })
    }
}
