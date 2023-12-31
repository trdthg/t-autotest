pub mod console;

pub use console::{SSHClient, SerialClient, VNCClient};
static MAGIC_STRING: &str = "n8acxy9o47xx7x7xw";

fn get_parsed_str_from_xt100_bytes(bytes: &[u8]) -> String {
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(bytes);
    parser.screen().contents()
}
