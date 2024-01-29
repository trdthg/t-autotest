mod evloop;
mod term;
mod vnc;

pub use evloop::*;
pub use term::*;
pub use vnc::{Rect, VNCClient, VNCError, VNCEventReq, VNCEventRes, PNG};

// magic string, used for regex extract in ssh or serial output
#[allow(dead_code)]
static MAGIC_STRING: &str = "n8acxy9o47xx7x7xw";
