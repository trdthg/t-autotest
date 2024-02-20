#[allow(unused)]
const LF: &str = "\n";
const CR: &str = "\r";
const CR_LF: &str = "\r\n";

pub trait Term {
    fn enter_input() -> &'static str {
        CR
    }

    fn enter_output() -> &'static str {
        CR_LF
    }

    fn linebreak() -> &'static str {
        Self::enter_output()
    }

    fn parse_and_strip(bytes: &[u8]) -> String {
        // bytes to string
        let text = String::from_utf8_lossy(bytes);
        // filter ESC and ANSI control character
        let text = console::strip_ansi_codes(&text);
        // Unicode control character shouldn't be filtered like \n, \u{7} (or BEL, or Ctrl-G)
        // text.chars().filter(|c| !c.is_control()).collect()
        text.to_string()
    }
}

struct General {}
impl Term for General {}

pub struct VT100 {}

impl Term for VT100 {
    fn parse_and_strip(bytes: &[u8]) -> String {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut res: String = String::new();
        for chunk in bytes.chunks(80 * 24) {
            parser.process(chunk);
            let contents = parser.screen().contents();
            res.push_str(contents.as_str());
        }
        let text = unescaper::unescape(&res).unwrap();
        let text = console::strip_ansi_codes(&text);
        text.to_string()
    }
}

pub struct VT102 {}

impl Term for VT102 {}

pub struct Xterm {}

impl Term for Xterm {}

#[cfg(test)]
mod test {
    use super::General;
    use crate::Term;

    #[test]
    fn test_default_parse() {
        for (src, expect) in [
            ("\n", "\n"), // unicode control character
            ("\u{7}", "\u{7}"), // same
            ("\u{1b}", "\u{1b}"),  // ANSI escape sequence, not complete
            ("\u{1b}[?2004l", ""), // same, but complete
            (" \u{1b}[32m", " "), // same
            (" \u{1b}[1;32mboard-image", " board-image"), // same
            (
                "echo $?W-x3JmwqB4C-h6yWhGTlk\r\n\u{1b}[?2004l\r0W-x3JmwqB4C-h6yWhGTlk\r\n\u{1b}[?2004hpi@raspberrypi:~$ ",
                "echo $?W-x3JmwqB4C-h6yWhGTlk\r\n\r0W-x3JmwqB4C-h6yWhGTlk\r\npi@raspberrypi:~$ "
            )
        ] {
            assert_eq!(General::parse_and_strip(src.as_bytes()), expect);
        }
    }
}
