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
use t_vnc::{client::Event, PixelFormat};
use tracing::{debug, error, info, trace, warn};

pub mod key {
    pub const BACK_SPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const RETURN: u32 = 0xff0d;
    pub const ENTER: u32 = RETURN;
    pub const ESCAPE: u32 = 0xff1b;
    pub const INSERT: u32 = 0xff63;
    pub const DELETE: u32 = 0xffff;
    pub const HOME: u32 = 0xff50;
    pub const END: u32 = 0xff57;
    pub const PAGE_UP: u32 = 0xff55;
    pub const PAGE_DOWN: u32 = 0xff56;
    pub const LEFT: u32 = 0xff51;
    pub const UP: u32 = 0xff52;
    pub const RIGHT: u32 = 0xff53;
    pub const DOWN: u32 = 0xff54;
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
    pub const SHIFT_L: u32 = 0xffe1;
    pub const SHIFT_R: u32 = 0xffe2;
    pub const CTRL_L: u32 = 0xffe3;
    pub const CTRL_R: u32 = 0xffe4;
    pub const META_L: u32 = 0xffe7;
    pub const META_R: u32 = 0xffe8;
    pub const ALT_L: u32 = 0xffe9;
    pub const ALT_R: u32 = 0xffea;
    pub const SUPER_L: u32 = 0xffeb;
    pub const SUPER_R: u32 = 0xffec;

    pub fn from_str(s: &str) -> Option<u32> {
        let key = match s {
            "back" | "backspace" => BACK_SPACE,
            "tab" => TAB,
            "ret" | "return" | "enter" => RETURN,
            "esc" | "escape" => ESCAPE,
            "ins" | "insert" => INSERT,
            "del" | "delete" => DELETE,
            "home" => HOME,
            "end" => END,
            "pageup" => PAGE_UP,
            "pagedown" => PAGE_DOWN,
            "left" => LEFT,
            "up" => UP,
            "right" => RIGHT,
            "down" => DOWN,
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
            "ctrl" | "ctrl_l" => CTRL_L,
            "ctrl_r" => CTRL_R,
            "shift" | "shift_l" => SHIFT_L,
            "shift_r" => SHIFT_R,
            "meta" | "meta_l" => META_L,
            "meta_r" => META_R,
            "alt" | "alt_l" => ALT_L,
            "alt_r" => ALT_R,
            "super" | "super_l" => SUPER_L,
            "super_r" => SUPER_R,
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
    MoveDown(u8),
    MoveUp(u8),
    MouseHide,
    TakeScreenShot,
    Refresh,
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
        let stream = TcpStream::connect(addr).map_err(VNCError::Io)?;

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
        screenshot_tx: Option<Sender<Container>>,
    ) -> Result<Self, VNCError> {
        let mut vnc = VNC::_connect(addr, password)?;

        // vnc.set_encodings(&[t_vnc::Encoding::Zrle, t_vnc::Encoding::DesktopSize])
        vnc.set_encodings(&[
            t_vnc::Encoding::Zrle,
            t_vnc::Encoding::CopyRect,
            t_vnc::Encoding::Raw,
            t_vnc::Encoding::Cursor,
            t_vnc::Encoding::DesktopSize,
        ])
        .unwrap();

        let size = vnc.size();
        let pixel_format = vnc.format();

        let (event_tx, event_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = channel();

        thread::spawn(move || {
            let mut c = VncClientInner {
                width: size.0,
                height: size.1,
                mouse_x: size.0,
                mouse_y: size.1,

                pixel_format,
                unstable_screen: Container::new(size.0, size.1, 3),
                updated_in_frame: false,
                last_take_screenshot: Instant::now(),

                buttons: 0,

                event_rx,
                stop_rx,

                screenshot_tx,
                screenshot_buffer: VecDeque::new(),
            };
            if let Err(e) = c.pool(&mut vnc) {
                error!(msg = "VNC stopped", reason=?e);
            }
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
    updated_in_frame: bool,

    buttons: u8,

    event_rx: Receiver<(VNCEventReq, Sender<VNCEventRes>)>,
    stop_rx: Receiver<()>,

    last_take_screenshot: Instant,
    screenshot_tx: Option<Sender<Container>>,
    screenshot_buffer: std::collections::VecDeque<PNG>,
}

impl VncClientInner {
    // vnc event loop
    fn pool(&mut self, vnc: &mut t_vnc::Client) -> Result<(), t_vnc::Error> {
        pub const FRAME_MS: u64 = 1000 / 60;

        info!(msg = "start event pool loop");

        vnc.request_update(
            Rect {
                left: 0,
                top: 0,
                width: self.width,
                height: self.height,
            },
            false,
        )?;

        'running: loop {
            if let Ok(()) = self.stop_rx.try_recv() {
                break;
            }

            let frame_start = std::time::Instant::now();
            let frame_end = frame_start.add(Duration::from_millis(FRAME_MS));

            trace!(msg = "handle vnc events");
            for event in vnc.poll_iter() {
                debug!(msg = "vnc receive new event");
                if let Err(e) = self.handle_vnc_event(event) {
                    if let Some(e) = e {
                        error!(msg="vnc disconnected unexpected", reason = ?e);
                        return Err(e);
                    }
                    break 'running;
                }
            }

            trace!(msg = "handle vnc req");
            loop {
                let action_start = Instant::now();
                if action_start > frame_end {
                    break;
                }
                if let Ok((msg, tx)) = self.event_rx.recv_timeout(frame_end - action_start) {
                    let res = self.handle_req(vnc, msg)?;
                    if tx.send(res).is_err() {
                        break;
                    };
                }
            }

            trace!(msg = "handle vnc update");
            vnc.request_update(
                Rect {
                    left: 0,
                    top: 0,
                    width: self.width,
                    height: self.height,
                },
                true,
            )?;
        }
        debug!(msg = "vnc stopped");
        Ok(())
    }

    fn handle_vnc_event(
        &mut self,
        event: t_vnc::client::Event,
    ) -> Result<(), Option<t_vnc::Error>> {
        match event {
            Event::Disconnected(e) => {
                return Err(e);
            }
            Event::Resize(w, h) => {
                info!(msg = "VNC Window Resize");
                self.updated_in_frame = true;
                self.width = w;
                self.height = h;
                let mut new_screen = Container::new(w, h, 3);
                new_screen.set_rect(0, 0, &self.unstable_screen);
                self.unstable_screen = new_screen;
            }
            Event::PutPixels(rect, pixels) => {
                self.updated_in_frame = true;
                let data = convert_to_rgb(&self.pixel_format, &pixels);
                let c = Container::new_with_data(rect.width, rect.height, data, 3);
                self.unstable_screen.set_rect(rect.left, rect.top, &c);
            }
            Event::CopyPixels { src, dst } => {
                self.updated_in_frame = true;
                self.unstable_screen.set_rect(
                    dst.left,
                    dst.top,
                    &Container::new_with_data(
                        dst.width,
                        dst.height,
                        self.unstable_screen.get_rect(src),
                        3,
                    ),
                );
            }
            Event::EndOfFrame => {
                if !self.updated_in_frame {
                    return Ok(());
                }

                // save buffer
                debug!(msg = "vnc event Event::EndOfFrame");
                if self.screenshot_buffer.len() == 120 {
                    self.screenshot_buffer.pop_front();
                }
                self.screenshot_buffer
                    .push_back(self.unstable_screen.clone());

                // send
                if let Some(tx) = &self.screenshot_tx {
                    if Instant::now().duration_since(self.last_take_screenshot)
                        > Duration::from_secs(2)
                    {
                        self.last_take_screenshot = Instant::now();
                        if tx.send(self.unstable_screen.clone()).is_err() {
                            error!(msg = "screenshot channel closed");
                            self.screenshot_tx = None;
                        }
                    }
                }

                self.updated_in_frame = false;
            }
            Event::Clipboard(ref _text) => {}
            Event::SetCursor { .. } => {}
            Event::SetColourMap { .. } => {}
            Event::Bell => {}
        }
        Ok(())
    }

    fn handle_req(
        &mut self,
        vnc: &mut t_vnc::Client,
        msg: VNCEventReq,
    ) -> Result<VNCEventRes, t_vnc::Error> {
        let res = match msg {
            VNCEventReq::TypeString(s) => {
                assert!(s.is_ascii());
                for c in s.as_bytes() {
                    let key = *c as u32;
                    vnc.send_key_event(true, key)?;
                    vnc.send_key_event(false, key)?;
                }
                VNCEventRes::Done
            }
            VNCEventReq::SendKey { keys } => {
                for m in keys.iter() {
                    vnc.send_key_event(true, *m)?;
                }
                for m in keys.iter().rev() {
                    vnc.send_key_event(false, *m)?;
                }
                VNCEventRes::Done
            }
            VNCEventReq::MouseMove(x, y) => {
                self.mouse_x = x;
                self.mouse_y = y;
                vnc.send_pointer_event(self.buttons, x, y)?;
                VNCEventRes::Done
            }
            VNCEventReq::MoveDown(button) => {
                self.buttons |= button;
                vnc.send_pointer_event(self.buttons, self.mouse_x, self.mouse_y)?;
                VNCEventRes::Done
            }
            VNCEventReq::MoveUp(button) => {
                self.buttons &= !button;
                vnc.send_pointer_event(self.buttons, self.mouse_x, self.mouse_y)?;
                VNCEventRes::Done
            }
            VNCEventReq::Refresh => {
                vnc.request_update(
                    Rect {
                        left: 0,
                        top: 0,
                        width: self.width,
                        height: self.height,
                    },
                    false,
                )?;
                VNCEventRes::Done
            }
            VNCEventReq::TakeScreenShot => {
                let screen = self
                    .screenshot_buffer
                    .back()
                    .unwrap_or(&Container::new(80, 24, 3))
                    .clone();
                VNCEventRes::Screen(screen)
            }
            VNCEventReq::MouseHide => {
                self.mouse_x = self.width;
                self.mouse_y = self.height;
                vnc.send_pointer_event(self.buttons, self.width, self.height)?;
                VNCEventRes::Done
            }
        };
        Ok(res)
    }
}

fn convert_to_rgb(pixel_format: &PixelFormat, raw_pixel_chunks: &[u8]) -> Vec<u8> {
    let byte_per_pixel = pixel_format.bits_per_pixel as usize / 8;
    let len = raw_pixel_chunks.len() / byte_per_pixel;

    let mut image_buffer: Vec<u8> = Vec::with_capacity(len * 3);

    // 将像素数据转换为图像缓冲区
    for pixel_chunk in raw_pixel_chunks.chunks_exact(byte_per_pixel) {
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
