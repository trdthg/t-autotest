use image::EncodableLayout;

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

    fn parse(bytes: &[u8]) -> String {
        let text = String::from_utf8_lossy(bytes.as_bytes()).to_string();
        let text = console::strip_ansi_codes(&text);
        unescaper::unescape(&text).unwrap()
    }
}

pub struct VT100 {}

impl Term for VT100 {
    fn parse(bytes: &[u8]) -> String {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut res: String = String::new();
        for chunk in bytes.chunks(80 * 24) {
            parser.process(chunk);
            let contents = parser.screen().contents();
            res.push_str(contents.as_str());
        }
        unescaper::unescape(&res).unwrap()
    }
}

pub struct VT102 {}

impl Term for VT102 {}

pub struct Xterm {}

impl Term for Xterm {}

#[cfg(test)]
mod test {
    use super::{Term, VT102};

    #[test]
    fn vt102() {
        for (src, expect) in [("\u{1b}[?2004l", ""),
            (" \u{1b}[32m", " "),
            (" \u{1b}[1;32mboard-image", " board-image"),
            (
                "echo $?W-x3JmwqB4C-h6yWhGTlk\r\n\u{1b}[?2004l\r0W-x3JmwqB4C-h6yWhGTlk\r\n\u{1b}[?2004hpi@raspberrypi:~$ ",
                "echo $?W-x3JmwqB4C-h6yWhGTlk\r\n\r0W-x3JmwqB4C-h6yWhGTlk\r\npi@raspberrypi:~$ "
            )] {
            assert_eq!(VT102::parse(src.as_bytes()), expect);
        }
    }
}
