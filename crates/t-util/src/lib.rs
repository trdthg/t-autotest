use std::{error::Error, fmt::Display, process::Command, sync::mpsc, thread, time::Duration};

use regex::Regex;
use tracing::{error, info, trace};

#[derive(Debug)]
pub enum RegexError {
    RegexBuildError(regex::Error),
}

pub fn assert_capture_between(
    src: &str,
    left: &str,
    right: &str,
) -> Result<Option<String>, RegexError> {
    let re = format!("(?s){}(.*){}", regex::escape(left), regex::escape(right));
    trace!(re = re, src = src, left = left, right = right,);
    let re = Regex::new(&re).map_err(RegexError::RegexBuildError)?;

    let mut locs = re.capture_locations();
    if re.captures_read(&mut locs, src).is_none() {
        return Ok(None);
    }
    let res_loc = locs.get(1).unwrap();

    Ok(Some(src[res_loc.0..res_loc.1].to_string()))
}

pub fn run_with_timeout<F, T>(f: F, timeout: Duration) -> Result<T, mpsc::RecvTimeoutError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    if timeout.is_zero() {
        return Ok(f());
    }

    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        trace!(msg = "run_with_timeout start");
        let result = f();
        if let Err(e) = sender.send(result) {
            error!(msg = "run_with_timeout send failed", reason = ?e);
        }
        info!(msg = "run_with_timeout done");
    });

    receiver.recv_timeout(timeout)
}

#[derive(Debug)]
pub enum ExecutorError {
    SpawnCommand(std::io::Error),
    WaitProcess(std::io::Error),
}
impl Error for ExecutorError {}
impl Display for ExecutorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutorError::SpawnCommand(e) => write!(f, "{}", e),
            ExecutorError::WaitProcess(e) => write!(f, "{}", e),
        }
    }
}

pub fn execute_shell(command: &str) -> Result<(), ExecutorError> {
    let mut cmd = Command::new("sh")
        .arg("-c")
        .arg(command)
        .spawn()
        .map_err(ExecutorError::SpawnCommand)?;

    cmd.wait().map_err(ExecutorError::WaitProcess)?;

    Ok(())
}

#[cfg(test)]
mod test {

    use std::process::Command;

    use super::*;

    #[test]
    fn test_exec_cmd() {
        let output = Command::new("bash")
            .args(["-c", "echo 1"])
            .output()
            .expect("Failed to execute command");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.to_string(), "1\n");
    }

    static MAGIC_STRING: &str = "n8acxy9o47xx7x7xw";

    #[test]
    fn test_regex() {
        let cmd = "whoami\n";
        let prompt = "pi@raspberrypi:~$ ";
        let src = "whoami\npi\npi@raspberrypi:~$ ";

        let res = assert_capture_between(src, cmd, prompt).unwrap().unwrap();
        println!("res: [{:?}]", res);
        assert!(res == "pi\n");
    }

    #[test]
    fn test_empty() {
        let cmd = "whoami\n";
        let prompt = "pi@raspberrypi:~$ ";
        let src = "whoami\npi@raspberrypi:~$ ";

        let res = assert_capture_between(src, cmd, prompt).unwrap().unwrap();
        println!("res: [{:?}]", res);
        assert!(res.is_empty());
    }

    #[test]
    fn test_escape() {
        let cmd = "export A=1\n";
        let prompt = "pi@raspberrypi:~$ ";
        let src = "export A=1\npi@raspberrypi:~$ ";

        let res = assert_capture_between(src, cmd, prompt).unwrap().unwrap();
        println!("res: [{:?}]", res);
        assert!(res.is_empty());
    }

    #[test]
    fn test_vt100_prompt() {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let src = include_bytes!("vt100.raw");
        parser.process(src);
        let src = parser.screen().contents();

        let prompt = assert_capture_between(
            &src,
            &format!("echo '{}'\n{}\n", MAGIC_STRING, MAGIC_STRING),
            "",
        )
        .unwrap()
        .unwrap();
        assert_eq!(prompt, "pi@raspberrypi:~$ ");
    }
}
