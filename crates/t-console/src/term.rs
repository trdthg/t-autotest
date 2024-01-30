use image::EncodableLayout;

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
        "\n"
    }

    fn parse(bytes: &[u8]) -> String {
        // let vt_regex = Regex::new(r"\x1b\[\??([\d]+(;)?)+[lhmk]").unwrap();
        // let text = String::from_utf8_lossy(bytes);
        // let cleaned_text = vt_regex.replace_all(&text, "");
        // cleaned_text.into_owned()
        let res = strip_ansi_escapes::strip(bytes);
        let text = String::from_utf8_lossy(res.as_bytes()).to_string();
        text
    }
}

pub struct Xterm {}

impl Term for Xterm {
    fn get_enter() -> &'static str {
        "\n"
    }

    fn parse(bytes: &[u8]) -> String {
        // let re = Regex::new(r"\x1B\[([0-9]{1,2}(;[0-9]{1,2})?)?[m|K]").unwrap();
        // re.replace_all(&text, "").to_string()
        let res = strip_ansi_escapes::strip(bytes);
        let text = String::from_utf8_lossy(res.as_bytes()).to_string();
        text
    }
}

#[cfg(test)]
mod test {
    use crate::Xterm;

    use super::{Term, VT102};

    #[test]
    fn vt102() {
        for (src, expect) in [("\u{1b}[?2004l", ""),
            (" \u{1b}[32m", " "),
            (" \u{1b}[1;32mboard-image", " board-image"),
            (
                "echo $?W-x3JmwqB4C-h6yWhGTlk\r\n\u{1b}[?2004l\r0W-x3JmwqB4C-h6yWhGTlk\r\n\u{1b}[?2004hpi@raspberrypi:~$ ",
                "echo $?W-x3JmwqB4C-h6yWhGTlk\n0W-x3JmwqB4C-h6yWhGTlk\npi@raspberrypi:~$ "
            )] {
            assert_eq!(VT102::parse(src.as_bytes()), expect);
        }
    }

    #[test]
    fn xterm() {
        {
            let (src, expect) = ("Linux revyos-pioneer 6.1.61-pioneer #2023.12.19.14.55+c60b48221 SMP Tue Dec 19 15:50:57 UTC 2023 riscv64\r\n\r\nThe programs included with the Debian GNU/Linux system are free software;\r\nthe exact distribution terms for each program are described in the\r\nindividual files in /usr/share/doc/*/copyright.\r\n\r\nDebian GNU/Linux comes with ABSOLUTELY NO WARRANTY, to the extent\r\npermitted by applicable law.\r\nLast login: Tue Jan 30 15:27:18 2024 from 192.168.33.248\r\r\n\u{1b}]0;debian@revyos-pioneer: ~\u{7}debian@revyos-pioneer:~$ tty; echo $?msZIEloAsQCe3h6LrEBfv\r\n\r/dev/pts/2\r\n0msZIEloAsQCe3h6LrEBfv\r\n\u{1b}]0;debian@revyos-pioneer: ~\u{7}debian@revyos-pioneer:~$ \u{1b}[K\r\n\r\u{1b}]0;debian@revyos-pioneer: ~\u{7}debian@revyos-pioneer:~$ cd && rm -rf $HOME/ruyitestfolder; echo $?juSSnkmKxvVLFDiINnJ2V\r\n\r0juSSnkmKxvVLFDiINnJ2V\r\n\u{1b}]0;debian@revyos-pioneer: ~\u{7}debian@revyos-pioneer:~$ \u{1b}[K\r\n\r\u{1b}]0;debian@revyos-pioneer: ~\u{7}debian@revyos-pioneer:~$ ",
             "Linux revyos-pioneer 6.1.61-pioneer #2023.12.19.14.55+c60b48221 SMP Tue Dec 19 15:50:57 UTC 2023 riscv64\n\nThe programs included with the Debian GNU/Linux system are free software;\nthe exact distribution terms for each program are described in the\nindividual files in /usr/share/doc/*/copyright.\n\nDebian GNU/Linux comes with ABSOLUTELY NO WARRANTY, to the extent\npermitted by applicable law.\nLast login: Tue Jan 30 15:27:18 2024 from 192.168.33.248\ndebian@revyos-pioneer:~$ tty; echo $?msZIEloAsQCe3h6LrEBfv\n/dev/pts/2\n0msZIEloAsQCe3h6LrEBfv\ndebian@revyos-pioneer:~$ \ndebian@revyos-pioneer:~$ cd && rm -rf $HOME/ruyitestfolder; echo $?juSSnkmKxvVLFDiINnJ2V\n0juSSnkmKxvVLFDiINnJ2V\ndebian@revyos-pioneer:~$ \ndebian@revyos-pioneer:~$ ");
            println!("{}", Xterm::parse(src.as_bytes()));
            assert_eq!(Xterm::parse(src.as_bytes()), expect);
        }
    }
}
