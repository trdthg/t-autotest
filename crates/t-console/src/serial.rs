use super::DuplexChannelConsole;
use crate::{parse_str_from_xt100_bytes, MAGIC_STRING};
use anyhow::Result;
use image::EncodableLayout;
use serialport::TTYPort;
use std::io::{self, Read, Write};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use t_util::ExecutorError;
use tracing::{debug, trace};

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

impl SerialClient {
    pub fn dump_history(&self) -> String {
        parse_str_from_xt100_bytes(&self.history)
    }

    pub fn connect(
        file: impl Into<String>,
        bund_rate: u32,
        timeout: Duration,
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

        res.conn.write_all("^C\n".as_bytes()).unwrap();
        sleep(Duration::from_millis(100));
        res.conn.flush().unwrap();

        Ok(res)
    }

    pub fn exec_global(&mut self, cmd: &str) -> Result<String> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        sleep(Duration::from_millis(70));
        let nanoid = nanoid::nanoid!();

        self.conn
            .write_all(format!("{}; echo {}\n", cmd, nanoid).as_bytes())
            .unwrap();

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
                Err(_) => unreachable!(),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::SerialClient;
    use std::{env, time::Duration};

    fn get_config_from_file() -> t_config::Config {
        let f = env::var("AUTOTEST_CONFIG_FILE").unwrap();
        t_config::load_config_from_file(f).unwrap()
    }

    fn get_client() -> SerialClient {
        let c = get_config_from_file();
        assert!(c.console.serial.enable);

        let c = c.console.serial;
        let serial =
            SerialClient::connect(c.serial_file, c.bund_rate, Duration::from_secs(1)).unwrap();
        serial
    }

    #[test]
    fn test_exec_global() {
        let mut serial = get_client();

        let cmds = vec![
            ("unset A", ""),
            (r#"echo "A=$A""#, "A=\n"),
            ("export A=1", ""),
            (r#"echo "A=$A""#, "A=1\n"),
        ];

        (0..10).for_each(|_| {
            for cmd in cmds.iter() {
                let res = serial.exec_global(cmd.0).unwrap();
                assert_eq!(res, cmd.1);
            }
        })
    }
}
