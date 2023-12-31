use regex::Regex;

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
    dbg!(&re);
    dbg!(&src);
    dbg!(&left);
    dbg!(&right);
    let re = Regex::new(&re).map_err(|e| RegexError::RegexBuildError(e))?;

    let mut locs = re.capture_locations();
    if re.captures_read(&mut locs, &src).is_none() {
        return Ok(None);
    }
    let res_loc = locs.get(1).unwrap();

    Ok(Some(src[res_loc.0..res_loc.1].to_string()))
}

#[cfg(test)]
mod test {

    use std::process::Command;

    use super::*;

    #[test]
    fn test_exec_cmd() {
        let output = Command::new("bash")
            .args(&["-c", "echo 1"])
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
        assert!(res == "");
    }

    #[test]
    fn test_escape() {
        let cmd = "export A=1\n";
        let prompt = "pi@raspberrypi:~$ ";
        let src = "export A=1\npi@raspberrypi:~$ ";

        let res = assert_capture_between(src, cmd, prompt).unwrap().unwrap();
        println!("res: [{:?}]", res);
        assert!(res == "");
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
