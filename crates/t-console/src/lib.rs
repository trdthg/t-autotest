mod event_loop;
mod serial;
mod ssh;
mod vnc;

pub use event_loop::*;
pub use serial::SerialClient;
pub use ssh::{SSHAuthAuth, SSHClient};
pub use vnc::{Rect, VNCClient, VNCError, VNCEventReq, VNCEventRes, PNG};

pub trait FullPowerConsole: ScreenControlConsole + DuplexChannelConsole {}

pub trait ScreenControlConsole {}

pub trait DuplexChannelConsole {}

// magic string, used for regex extract in ssh or serial output
#[allow(dead_code)]
static MAGIC_STRING: &str = "n8acxy9o47xx7x7xw";

// get display string from raw xt100 output
fn parse_str_from_vt100_bytes(bytes: &[u8]) -> String {
    let mut res = String::new();
    for chunk in bytes.chunks(80 * 24) {
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(chunk);
        let contents = parser.screen().contents();
        res.push_str(contents.as_str());
    }
    res
}

#[cfg(test)]
mod test {}
