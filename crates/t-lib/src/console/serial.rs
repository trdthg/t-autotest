use std::io::{self, Read, Write};
use std::time::Duration;

use serialport::SerialPort;

use crate::{get_parsed_str_from_xt100_bytes, MAGIC_STRING};

use super::DuplexChannelConsole;

pub struct SerialClient {
    conn: Box<dyn SerialPort>,
    prompt: String,
}

impl DuplexChannelConsole for SerialClient {
    fn exec(&mut self, cmd: &str) -> String {
        self.exec(cmd).unwrap()
    }
}

#[derive(Debug)]
pub enum SerialError {
    ConnectError(String),
    Read(io::Error),
    Write(io::Error),
}

impl SerialClient {
    pub fn connect(
        file: impl Into<String>,
        bund_rate: u32,
        timeout: Duration,
    ) -> Result<Self, SerialError> {
        let file = file.into();
        let port = serialport::new(&file, bund_rate)
            .timeout(timeout)
            .open()
            .map_err(|e| SerialError::ConnectError(e.to_string()))?;

        let mut res = Self {
            conn: port,
            prompt: "".to_string(),
        };

        res.update_prompt();

        Ok(res)
    }

    fn write_str(&mut self, cmd: &str) -> Result<(), SerialError> {
        let serial = &mut self.conn;
        serial
            .write(format!("{}", cmd).as_bytes())
            .map_err(|e| SerialError::Write(e))?;
        Ok(())
    }

    fn read_raw(&mut self) -> Result<Vec<u8>, SerialError> {
        let serial = &mut self.conn;
        let mut res = Vec::new();
        serial
            .read_to_end(&mut res)
            .map_err(|e| SerialError::Read(e))?;
        Ok(res)
    }

    fn run_cmd(&mut self, cmd: &str) -> Result<String, SerialError> {
        self.write_str(&format!("{}\n", cmd))?;
        let res = self.read_raw()?;
        return Ok(get_parsed_str_from_xt100_bytes(&res));
    }

    pub fn exec(&mut self, cmd: &str) -> Result<String, SerialError> {
        let res = self.run_cmd(cmd)?;
        let res = t_util::assert_capture_between(&res, &format!("{}\n", cmd), &self.prompt)
            .unwrap()
            .unwrap();
        Ok(res)
    }

    fn update_prompt(&mut self) {
        let res = self.run_cmd(&format!("echo '{}'", MAGIC_STRING)).unwrap();

        let prompt = t_util::assert_capture_between(
            &res,
            &format!("echo '{}'\n{}\n", MAGIC_STRING, MAGIC_STRING),
            "",
        )
        .unwrap()
        .unwrap();

        self.prompt = prompt;
    }
}

#[cfg(test)]
mod test {
    use std::env;

    use super::*;

    #[test]
    fn test_wr() {
        let file = env::var("SERIAL_FILE").unwrap_or("/dev/ttyUSB0".to_string());
        let mut serial = SerialClient::connect(file, 115_200, Duration::from_secs(1)).unwrap();

        let cmds = vec![("export A=1", ""), (r#"echo "A=$A""#, "A=1\n")];
        for cmd in cmds {
            let res = serial.exec(cmd.0).unwrap();
            assert_eq!(res, cmd.1);
        }
    }
}
