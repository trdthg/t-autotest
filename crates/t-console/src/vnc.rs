mod data;

use std::{
    collections::VecDeque,
    error::Error,
    fmt::Display,
    io,
    net::{SocketAddr, TcpStream},
    ops::Add,
    sync::mpsc::{self, channel, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use data::Container;
pub use data::Rect;
use t_vnc::PixelFormat;
use tracing::{debug, error, info, trace, warn};

pub mod Key {
    pub const BackSpace: u32 = 0xff08;
    pub const Tab: u32 = 0xff09;
    pub const Return: u32 = 0xff0d;
    pub const Enter: u32 = Return;
    pub const Escape: u32 = 0xff1b;
    pub const Insert: u32 = 0xff63;
    pub const Delete: u32 = 0xffff;
    pub const Home: u32 = 0xff50;
    pub const End: u32 = 0xff57;
    pub const PageUp: u32 = 0xff55;
    pub const PageDown: u32 = 0xff56;
    pub const Left: u32 = 0xff51;
    pub const Up: u32 = 0xff52;
    pub const Right: u32 = 0xff53;
    pub const Down: u32 = 0xff54;
    pub const F1: u32 = 0xffbe;
    pub const F2: u32 = 0xffbf;
    pub const F3: u32 = 0xffc0;
    pub const F4: u32 = 0xffc1;
    pub const F5: u32 = 0xffc2;
    pub const F6: u32 = 0xffc3;
    pub const F7: u32 = 0xffc4;
    pub const F8: u32 = 0xffc5;
    pub const F9: u32 = 0xffc6;
    pub const F10: u32 = 0xffc7;
    pub const F11: u32 = 0xffc8;
    pub const F12: u32 = 0xffc9;
    pub const ShiftLeft: u32 = 0xffe1;
    pub const ShiftRight: u32 = 0xffe2;
    pub const CtrlLeft: u32 = 0xffe3;
    pub const CtrlRight: u32 = 0xffe4;
    pub const MetaLeft: u32 = 0xffe7;
    pub const MetaRight: u32 = 0xffe8;
    pub const AltLeft: u32 = 0xffe9;
    pub const AltRight: u32 = 0xffea;

    pub fn from_str(s: &str) -> Option<u32> {
        let key = match s {
            "back" | "backspace" => BackSpace,
            "tab" => Tab,
            "ret" | "return" | "enter" => Return,
            "esc" | "escape" => Escape,
            "ins" | "insert" => Insert,
            "del" | "delete" => Delete,
            "home" => Home,
            "end" => End,
            "pageup" => PageUp,
            "pagedown" => PageDown,
            "left" => Left,
            "up" => Up,
            "right" => Right,
            "down" => Down,
            "f1" => F1,
            "f2" => F2,
            "f3" => F3,
            "f4" => F4,
            "f5" => F5,
            "f6" => F6,
            "f7" => F7,
            "f8" => F8,
            "f9" => F9,
            "f10" => F10,
            "f11" => F11,
            "f12" => F12,
            "ctrl" | "ctrll" | "lctrl" => CtrlLeft,
            "rctrl" | "ctrlr" => CtrlRight,
            "shift" | "shiftl" | "lshift" => ShiftLeft,
            "shiftr" | "rshift" => ShiftRight,
            "meta" | "metal" | "lmeta" => MetaLeft,
            "rmeta" | "metar" => MetaRight,
            "alt" | "altl" | "lalt" => AltLeft,
            "altr" | "ralt" => AltRight,
            _ => 0,
        };
        if key == 0 {
            let bytes = s.as_bytes();
            if bytes.len() == 1 && bytes[0].is_ascii() {
                return Some(bytes[0] as u32);
            }
            None
        } else {
            Some(key)
        }
    }
}

pub enum VNCEventReq {
    TypeString(String),
    SendKey { keys: Vec<u32> },
    MouseMove(u16, u16),
    MoveDown,
    MoveUp,
    MouseHide,
    TakeScreenShot,
}

pub type PNG = Container;

pub enum VNCEventRes {
    Done,
    Screen(PNG),
}

pub struct VNC {
    pub event_tx: Sender<(VNCEventReq, Sender<VNCEventRes>)>,
    pub stop_tx: Sender<()>,
}

#[derive(Debug)]
pub enum VNCError {
    VNCError(t_vnc::Error),
    Io(io::Error),
}
impl Error for VNCError {}
impl Display for VNCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VNCError::VNCError(e) => write!(f, "{}", e),
            VNCError::Io(e) => write!(f, "{}", e),
        }
    }
}

impl VNC {
    fn _connect(addr: SocketAddr, password: Option<String>) -> Result<t_vnc::Client, VNCError> {
        let stream =
            TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(VNCError::Io)?;

        let vnc = t_vnc::Client::from_tcp_stream(stream, false, |methods| {
            for method in methods {
                match method {
                    t_vnc::client::AuthMethod::None => {
                        return Some(t_vnc::client::AuthChoice::None)
                    }
                    t_vnc::client::AuthMethod::Password => {
                        return match password {
                            None => None,
                            Some(password) => {
                                let mut key = [0; 8];
                                for (i, byte) in password.bytes().enumerate() {
                                    if i == 8 {
                                        break;
                                    }
                                    key[i] = byte
                                }
                                Some(t_vnc::client::AuthChoice::Password(key))
                            }
                        }
                    }
                    m => {
                        warn!(msg = "unimplemented", method = ?m);
                        continue;
                    }
                }
            }
            None
        })
        .map_err(VNCError::VNCError)?;
        Ok(vnc)
    }

    pub fn connect(
        addr: SocketAddr,
        password: Option<String>,
        screenshot_tx: Option<Sender<PNG>>,
    ) -> Result<Self, VNCError> {
        let mut vnc = VNC::_connect(addr, password)?;
        let size = vnc.size();
        let pixel_format = vnc.format();

        let (event_tx, event_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = channel();

        thread::spawn(move || {
            VncClientInner {
                width: size.0,
                height: size.1,
                mouse_x: size.0,
                mouse_y: size.1,
                pixel_format,
                unstable_screen: Container::new(size.0, size.1, 3),

                event_rx,
                stop_rx,

                screenshot_tx,
                screenshot_buffer: VecDeque::new(),
            }
            .pool(&mut vnc);
        });

        Ok(Self { event_tx, stop_tx })
    }

    pub fn stop(&self) {
        if self.stop_tx.send(()).is_err() {
            error!("vnc stopped failed")
        }
    }
}

struct VncClientInner {
    width: u16,
    height: u16,
    mouse_x: u16,
    mouse_y: u16,

    pixel_format: PixelFormat,
    unstable_screen: Container,

    event_rx: Receiver<(VNCEventReq, Sender<VNCEventRes>)>,
    stop_rx: Receiver<()>,
    screenshot_tx: Option<Sender<PNG>>,

    screenshot_buffer: std::collections::VecDeque<PNG>,
}

impl VncClientInner {
    // vnc event loop
    fn pool(&mut self, vnc: &mut t_vnc::Client) {
        pub const FRAME_MS: u64 = 1000 / 60;

        let mut last_send = Instant::now();

        info!(msg = "start event pool loop");
        'running: loop {
            if let Ok(()) = self.stop_rx.try_recv() {
                break;
            }

            let start = std::time::Instant::now();

            trace!(msg = "vnc waiting new events");
            for event in vnc.poll_iter() {
                debug!(msg = "vnc receive new event");
                use t_vnc::client::Event;
                match event {
                    Event::Disconnected(None) => {
                        break 'running;
                    }
                    Event::Disconnected(Some(error)) => {
                        error!("server disconnected: {:?}", error);
                        break 'running;
                    }
                    Event::Resize(w, h) => {
                        self.width = w;
                        self.height = h;
                        let mut new_screen = Container::new(w, h, 3);
                        new_screen.set_rect(0, 0, self.unstable_screen.clone());
                        self.unstable_screen = new_screen;
                    }
                    Event::PutPixels(rect, pixels) => {
                        let data = convert_to_rgb(&self.pixel_format, &pixels);
                        let c = Container::new_with_data(rect.width, rect.height, data, 3);
                        self.unstable_screen.set_rect(rect.left, rect.top, c);
                    }
                    Event::CopyPixels { src, dst } => {
                        let data = self.unstable_screen.get_rect(src);
                        self.unstable_screen.set_rect(
                            dst.left,
                            dst.top,
                            Container::new_with_data(dst.width, dst.height, data, 3),
                        );
                    }
                    Event::EndOfFrame => {
                        debug!(msg = "vnc event Event::EndOfFrame");
                        if self.screenshot_buffer.len() == 120 {
                            self.screenshot_buffer.pop_front();
                        }
                        self.screenshot_buffer
                            .push_back(self.unstable_screen.clone());

                        if let Some(ref tx) = self.screenshot_tx {
                            if let Some(screenshot) = self.screenshot_buffer.pop_front() {
                                if tx.send(screenshot).is_err() {
                                    self.screenshot_tx = None;
                                    info!(msg = "vnc client stopped");
                                }
                            }
                            last_send = Instant::now();
                        }
                    }
                    Event::Clipboard(ref _text) => {}
                    Event::SetCursor { .. } => {}
                    _ => unimplemented!(), /* ignore unsupported events */
                }
            }

            let timeout = || {
                std::time::Instant::now()
                    .duration_since(start)
                    .add(Duration::from_millis(FRAME_MS))
            };

            let mouse_buttons_mask = 0u8;
            let mouse_button_left = 0x01;
            while let Ok((msg, tx)) = self.event_rx.recv_timeout(timeout()) {
                let res = match msg {
                    VNCEventReq::TypeString(s) => {
                        for c in s.as_bytes() {
                            if c.is_ascii() {
                                let key = *c as u32;
                                vnc.send_key_event(true, key);
                                vnc.send_key_event(false, key);
                            }
                        }
                        VNCEventRes::Done
                    }
                    VNCEventReq::SendKey { keys } => {
                        for m in keys.iter() {
                            vnc.send_key_event(true, *m);
                        }
                        for m in keys.iter().rev() {
                            vnc.send_key_event(false, *m);
                        }
                        VNCEventRes::Done
                    }
                    VNCEventReq::MouseMove(x, y) => {
                        debug!(msg = "mouse move", x = x, y = y);
                        if vnc.send_pointer_event(mouse_buttons_mask, x, y).is_err() {
                            break;
                        };
                        self.mouse_x = x;
                        self.mouse_y = y;
                        VNCEventRes::Done
                    }
                    VNCEventReq::MoveDown => {
                        let mouse_button = mouse_buttons_mask | mouse_button_left;
                        if vnc
                            .send_pointer_event(mouse_button, self.mouse_x, self.mouse_y)
                            .is_err()
                        {
                            break;
                        };
                        VNCEventRes::Done
                    }
                    VNCEventReq::MoveUp => {
                        let mouse_button = mouse_buttons_mask & !mouse_button_left;
                        if vnc
                            .send_pointer_event(mouse_button, self.mouse_x, self.mouse_y)
                            .is_err()
                        {
                            break;
                        };
                        VNCEventRes::Done
                    }
                    VNCEventReq::TakeScreenShot => {
                        let screen = self.screenshot_buffer[0].clone();
                        VNCEventRes::Screen(screen)
                    }
                    VNCEventReq::MouseHide => {
                        if vnc
                            .send_pointer_event(mouse_button_left, self.width, self.height)
                            .is_err()
                        {
                            break;
                        };
                        self.mouse_x = self.width;
                        self.mouse_y = self.height;
                        VNCEventRes::Done
                    }
                };
                if tx.send(res).is_err() {
                    break;
                };
            }
            if vnc
                .request_update(
                    Rect {
                        left: 0,
                        top: 0,
                        width: self.width,
                        height: self.height,
                    },
                    true,
                )
                .is_err()
            {
                break;
            }
        }
        debug!(msg = "vnc stopped");
    }
}

fn convert_to_rgb(pixel_format: &PixelFormat, raw_pixel_chunks: &[u8]) -> Vec<u8> {
    let byte_per_pixel = pixel_format.bits_per_pixel as usize / 8;
    let len = raw_pixel_chunks.len() / byte_per_pixel;
    let mut image_buffer: Vec<u8> = Vec::new();

    // 将像素数据转换为图像缓冲区
    for i in 0..len {
        let pixel_chunk = &raw_pixel_chunks[(i * byte_per_pixel)..((i + 1) * byte_per_pixel)];
        let pixel_value = if pixel_format.big_endian {
            BigEndian::read_u32(pixel_chunk)
        } else {
            LittleEndian::read_u32(pixel_chunk)
        };

        let red_mask = pixel_format.red_max as u32;
        let green_mask = pixel_format.green_max as u32;
        let blue_mask = pixel_format.blue_max as u32;

        let red = (pixel_value >> pixel_format.red_shift & red_mask) as u8;
        let green = (pixel_value >> pixel_format.green_shift & green_mask) as u8;
        let blue = (pixel_value >> pixel_format.blue_shift & blue_mask) as u8;

        image_buffer.push(red);
        image_buffer.push(green);
        image_buffer.push(blue);
    }

    image_buffer
}
