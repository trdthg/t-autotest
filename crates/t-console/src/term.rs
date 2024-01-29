use regex::Regex;

pub trait Term {
    fn get_enter() -> &'static str;
    fn parse(bytes: &[u8]) -> String;
}

pub struct VT100 {}

impl Term for VT100 {
    fn get_enter() -> &'static str {
        "\n"
    }

    fn parse(bytes: &[u8]) -> String {
        let mut parser = vt100::Parser::new(24, 80, 0);
        let mut res: String = String::new();
        for chunk in bytes.chunks(80 * 24) {
            parser.process(chunk);
            let contents = parser.screen().contents();
            res.push_str(contents.as_str());
        }
        res
    }
}

pub struct VT102 {}

impl Term for VT102 {
    fn get_enter() -> &'static str {
        "\r\n"
    }

    fn parse(bytes: &[u8]) -> String {
        let vt_regex = Regex::new(r"\x1b\[\??([\d]+(;)?)+[lhmk](\r\n)?").unwrap();
        let text = String::from_utf8_lossy(bytes);
        let cleaned_text = vt_regex.replace_all(&text, "");
        cleaned_text.into_owned()
    }
}

#[cfg(test)]
mod test {
    use super::{Term, VT102};

    #[test]
    fn vt102() {
        for (src, expect) in vec![
            ("\u{1b}[?2004l", ""),
            (" \u{1b}[32m", " "),
            (" \u{1b}[1;32mboard-image", " board-image"),
            (
                "echo $?W-x3JmwqB4C-h6yWhGTlk\r\n\u{1b}[?2004l\r0W-x3JmwqB4C-h6yWhGTlk\r\n\u{1b}[?2004hpi@raspberrypi:~$ ",
                "echo $?W-x3JmwqB4C-h6yWhGTlk\r\n\r0W-x3JmwqB4C-h6yWhGTlk\r\npi@raspberrypi:~$ "
            ),
        ] {
            let s = VT102::parse(src.as_bytes());
            assert_eq!(s, expect);
        }
    }
}
