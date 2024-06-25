mod data;

use std::{
    collections::VecDeque,
    error::Error,
    fmt::Display,
    io,
    net::{SocketAddr, TcpStream},
    sync::{
        mpsc::{self, channel, Receiver, RecvError, RecvTimeoutError, Sender},
        Arc,
    },
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
        let key = match s.to_lowercase().as_str() {
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

#[derive(Debug)]
pub enum VNCEventReq {
    TypeString(String),
    SendKey { keys: Vec<u32> },
    MouseMove(u16, u16),
    MouseDrag(u16, u16),
    MouseClick(u8),
    MoveDown(u8),
    MoveUp(u8),
    MouseHide,
    GetScreenShot,
    TakeScreenShot(String, Option<String>),
    Refresh,
}

pub type PNG = Container;

pub enum VNCEventRes {
    NoConnection,
    Done,
    Screen(Arc<PNG>),
}

pub struct VNC {
    pub event_tx: Sender<(VNCEventReq, Sender<VNCEventRes>)>,
    pub stop_tx: Sender<Sender<()>>,
}

pub enum Log {
    Screenshot {
        screen: Arc<PNG>,
        name: String,
        span: Option<String>,
        done_tx: Sender<()>,
    },
}

pub type LogTx = Sender<Log>;

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
    fn make_conn(addr: &SocketAddr, password: Option<String>) -> Result<t_vnc::Client, VNCError> {
        let stream =
            TcpStream::connect_timeout(addr, Duration::from_millis(200)).map_err(VNCError::Io)?;

        let mut vnc = t_vnc::Client::from_tcp_stream(stream, true, |methods| {
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

        // vnc.set_encodings(&[t_vnc::Encoding::Zrle, t_vnc::Encoding::DesktopSize])
        vnc.set_encodings(&[
            t_vnc::Encoding::Zrle,
            t_vnc::Encoding::CopyRect,
            t_vnc::Encoding::Raw,
            t_vnc::Encoding::Cursor,
            t_vnc::Encoding::DesktopSize,
        ])
        .map_err(VNCError::VNCError)?;

        info!(msg = "vnc connect success");

        Ok(vnc)
    }

    pub fn connect(
        addr: SocketAddr,
        password: Option<String>,
        screenshot_tx: Option<LogTx>,
    ) -> Result<Self, VNCError> {
        let vnc = Self::make_conn(&addr, password.clone())?;

        let (event_tx, event_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = channel();

        let mut c = VncClientInner {
            make_conn: Box::new(move || Self::make_conn(&addr, password.clone())),
            state: State::from_vnc(&vnc),
            conn: Some(vnc),

            event_rx,
            stop_rx,

            screenshot_tx,
            screenshot_buffer: VecDeque::new(),
        };

        thread::spawn(move || {
            if let Err(e) = c.pool() {
                error!(msg = "VNC stopped", reason=?e);
            }
        });

        Ok(Self { event_tx, stop_tx })
    }

    pub fn send(&self, req: VNCEventReq) -> Result<VNCEventRes, RecvError> {
        let (tx, rx) = mpsc::channel();
        if self.event_tx.send((req, tx)).is_err() {
            warn!("vnc client stopped unexpected");
            return Err(RecvError);
        }
        rx.recv()
    }

    pub fn send_timeout(
        &self,
        req: VNCEventReq,
        timeout: Duration,
    ) -> Result<VNCEventRes, RecvTimeoutError> {
        let (tx, rx) = mpsc::channel();
        if self.event_tx.send((req, tx)).is_err() {
            panic!("vnc client stopped unexpected")
        }
        rx.recv_timeout(timeout)
    }

    pub fn stop(&self) {
        let (tx, rx) = channel();
        if self.stop_tx.send(tx).is_err() {
            error!("vnc stopped failed")
        }
        if let Err(e) = rx.recv() {
            error!(msg = "vnc stop failed", reason=?e);
        }
    }
}

type MakeVncConn = Box<dyn Fn() -> Result<t_vnc::Client, VNCError> + Send + 'static>;

struct State {
    width: u16,
    height: u16,
    mouse_x: u16,
    mouse_y: u16,

    count: i32,

    pixel_format: PixelFormat,
    unstable_screen: Container,
    updated_in_frame: bool,

    buttons: u8,
}

impl State {
    fn from_vnc(vnc: &t_vnc::Client) -> Self {
        let size = &vnc.size();
        let pixel_format = vnc.format();
        Self {
            width: size.0,
            height: size.1,
            mouse_x: size.0,
            mouse_y: size.1,

            count: 0,

            pixel_format,
            unstable_screen: Container::new(size.0, size.1, 3),
            updated_in_frame: true,
            buttons: 0,
        }
    }
}

struct VncClientInner {
    make_conn: MakeVncConn,
    conn: Option<t_vnc::Client>,

    state: State,

    event_rx: Receiver<(VNCEventReq, Sender<VNCEventRes>)>,
    stop_rx: Receiver<Sender<()>>,

    screenshot_tx: Option<LogTx>,
    screenshot_buffer: std::collections::VecDeque<Arc<PNG>>,
}

impl VncClientInner {
    // vnc event loop
    fn pool(&mut self) -> Result<(), t_vnc::Error> {
        const FRAME_MS: u64 = 1000 / 60;

        info!(msg = "start event pool loop");

        loop {
            // handle return
            if let Ok(tx) = self.stop_rx.try_recv() {
                tx.send(()).ok();
                break;
            }

            // handle reconnect
            if self.conn.is_none() {
                if let Ok(vnc) = self.make_conn.as_ref()() {
                    self.state = State::from_vnc(&vnc);
                    self.conn = Some(vnc);
                }
            };

            // request refresh
            if let Some(vnc) = self.conn.as_mut() {
                trace!(msg = "handle vnc update");
                let _ = vnc.request_update(
                    Rect {
                        left: 0,
                        top: 0,
                        width: self.state.width,
                        height: self.state.height,
                    },
                    true,
                );
            }

            let deadline = Instant::now() + Duration::from_millis(FRAME_MS);
            // handle server events
            trace!(msg = "handle vnc events");
            while let Some(event) = self.conn.as_mut().and_then(|vnc| vnc.poll_event()) {
                debug!(msg = "vnc receive new event");
                if let Err(e) = self.try_handle_vnc_events(event) {
                    error!(msg="vnc disconnected", reason = ?e);
                    self.conn = None;
                    break;
                }
            }

            // handle user requests
            trace!(msg = "handle vnc req");
            while let Ok((msg, tx)) = self.event_rx.try_recv() {
                // info!(msg="handle new msg", req=?msg);
                match self.handle_req(msg) {
                    Ok(res) => {
                        if tx.send(res).is_err() {
                            error!(msg = "vnc event result send back failed");
                        };
                    }
                    Err(_) => {
                        if tx.send(VNCEventRes::NoConnection).is_err() {
                            self.conn = None;
                            error!(msg = "vnc connection may broken, close connection");
                        };
                    }
                }
                if Instant::now() > deadline {
                    break;
                }
            }
            if Instant::now() < deadline {
                thread::sleep(deadline - Instant::now());
            }
        }
        debug!(msg = "vnc stopped");
        Ok(())
    }

    fn try_handle_vnc_events(
        &mut self,
        event: t_vnc::client::Event,
    ) -> Result<(), Option<t_vnc::Error>> {
        let Self { state, .. } = self;
        match event {
            Event::Disconnected(e) => {
                state.updated_in_frame = true;
                state.unstable_screen.set_zero();
                let screenshot = Arc::new(state.unstable_screen.clone());
                self.screenshot_buffer.push_back(screenshot.clone());
                return Err(e);
            }
            Event::Resize(w, h) => {
                info!(msg = "VNC Window Resize", w = w, h = h);
                state.updated_in_frame = true;
                state.width = w;
                state.height = h;
                let mut new_screen = Container::new(w, h, 3);
                new_screen.set_rect(0, 0, &state.unstable_screen);
                state.unstable_screen = new_screen;
            }
            Event::PutPixels(rect, pixels) => {
                if !pixels.is_empty() {
                    state.updated_in_frame = true;
                }
                let data = convert_to_rgb(&state.pixel_format, &pixels);
                let c = Container::new_with_data(rect.width, rect.height, data, 3);
                state.unstable_screen.set_rect(rect.left, rect.top, &c);
            }
            Event::CopyPixels { src, dst } => {
                if src != dst {
                    state.updated_in_frame = true;
                }
                state.unstable_screen.set_rect(
                    dst.left,
                    dst.top,
                    &Container::new_with_data(
                        dst.width,
                        dst.height,
                        state.unstable_screen.get_rect(src),
                        3,
                    ),
                );
            }
            Event::EndOfFrame => {
                if !state.updated_in_frame {
                    return Ok(());
                }
                state.count += 1;
                state.updated_in_frame = false;

                // save buffer
                debug!(msg = "vnc event Event::EndOfFrame", count = state.count);
                while self.screenshot_buffer.len() > 10 {
                    self.screenshot_buffer.pop_front();
                }

                let screenshot = Arc::new(state.unstable_screen.clone());
                self.screenshot_buffer.push_back(screenshot.clone());

                // FIXME: send screenshot may cause memoey overflow slowly if handler handle too slow
                // if let Some(tx) = &self.screenshot_tx {
                //     // if let Some(last) = self.last_take_screenshot {
                //     //     // TODO: maybe set a minimal interval
                //     //     // if Instant::now().duration_since(last) < Duration::from_secs(2) {
                //     //     //     continue
                //     //     // }
                //     // }
                //     if tx.send(screenshot).is_err() {
                //         error!(msg = "screenshot channel closed");
                //         self.screenshot_tx = None;
                //     }
                //     self.last_take_screenshot = Some(Instant::now());
                // }
            }
            Event::Clipboard(ref _text) => {
                state.updated_in_frame = true;
            }
            Event::SetCursor { .. } => {
                state.updated_in_frame = true;
            }
            Event::SetColourMap { .. } => {
                state.updated_in_frame = true;
            }
            Event::Bell => {
                state.updated_in_frame = true;
            }
        }
        Ok(())
    }

    fn handle_req(&mut self, msg: VNCEventReq) -> Result<VNCEventRes, t_vnc::Error> {
        match msg {
            VNCEventReq::TypeString(s) => self.handle_type_string(s),
            VNCEventReq::SendKey { keys } => self.handle_send_key(keys),
            VNCEventReq::MouseMove(x, y) => self.handle_mouse_move(x, y),
            VNCEventReq::MouseDrag(x, y) => self.handle_mouse_drag(x, y),
            VNCEventReq::MouseClick(button) => {
                self.handle_mouse_down(button)?;
                self.handle_mouse_up(button)?;
                Ok(VNCEventRes::Done)
            }
            VNCEventReq::MoveDown(button) => self.handle_mouse_down(button),
            VNCEventReq::MoveUp(button) => self.handle_mouse_up(button),
            VNCEventReq::Refresh => self.handle_screen_refresh(),
            VNCEventReq::GetScreenShot => self.handle_screen_getlatest(),
            VNCEventReq::TakeScreenShot(name, span) => self.handle_screen_takeshot(name, span),
            VNCEventReq::MouseHide => self.handle_mouse_hide(),
        }
    }

    fn handle_mouse_down(&mut self, button: u8) -> Result<VNCEventRes, t_vnc::Error> {
        if let Some(vnc) = self.conn.as_mut() {
            let new_buttons = self.state.buttons | button;
            vnc.send_pointer_event(new_buttons, self.state.mouse_x, self.state.mouse_y)?;
            self.state.buttons = new_buttons;
            return Ok(VNCEventRes::Done);
        }
        Ok(VNCEventRes::NoConnection)
    }
    fn handle_mouse_up(&mut self, button: u8) -> Result<VNCEventRes, t_vnc::Error> {
        if let Some(vnc) = self.conn.as_mut() {
            let new_buttons = self.state.buttons & !button;
            vnc.send_pointer_event(new_buttons, self.state.mouse_x, self.state.mouse_y)?;
            self.state.buttons = new_buttons;
            return Ok(VNCEventRes::Done);
        }
        Ok(VNCEventRes::NoConnection)
    }

    fn handle_mouse_move(&mut self, x: u16, y: u16) -> Result<VNCEventRes, t_vnc::Error> {
        if !self.check_move(x, y) {
            return Ok(VNCEventRes::Done);
        }
        if let Some(vnc) = self.conn.as_mut() {
            vnc.send_pointer_event(self.state.buttons, x, y)?;
            self.state.mouse_x = x;
            self.state.mouse_y = y;
            return Ok(VNCEventRes::Done);
        }
        Ok(VNCEventRes::NoConnection)
    }

    fn handle_mouse_hide(&mut self) -> Result<VNCEventRes, t_vnc::Error> {
        if let Some(vnc) = self.conn.as_mut() {
            vnc.send_pointer_event(self.state.buttons, self.state.width, self.state.height)?;
            self.state.mouse_x = self.state.width;
            self.state.mouse_y = self.state.height;
            return Ok(VNCEventRes::Done);
        }
        Ok(VNCEventRes::NoConnection)
    }

    fn check_move(&self, x: u16, y: u16) -> bool {
        self.state.mouse_x != x || self.state.mouse_y != y
    }

    fn handle_mouse_drag(&mut self, x: u16, y: u16) -> Result<VNCEventRes, t_vnc::Error> {
        if !self.check_move(x, y) {
            return Ok(VNCEventRes::Done);
        }
        for i in self.state.mouse_x..self.state.mouse_x + x {
            self.handle_mouse_move(
                i,
                i * self.state.mouse_y
                    + y / if self.state.mouse_x + x == 0 {
                        0
                    } else {
                        self.state.mouse_x + x
                    },
            )?;
        }
        self.handle_mouse_move(x, y)
    }

    fn handle_send_key(&mut self, keys: Vec<u32>) -> Result<VNCEventRes, t_vnc::Error> {
        if let Some(vnc) = self.conn.as_mut() {
            for m in keys.iter() {
                vnc.send_key_event(true, *m)?;
            }
            for m in keys.iter().rev() {
                vnc.send_key_event(false, *m)?;
            }
            return Ok(VNCEventRes::Done);
        }
        Ok(VNCEventRes::NoConnection)
    }

    fn handle_type_string(&mut self, s: String) -> Result<VNCEventRes, t_vnc::Error> {
        assert!(s.is_ascii());
        if let Some(vnc) = self.conn.as_mut() {
            for c in s.as_bytes() {
                let key = *c as u32;
                vnc.send_key_event(true, key)?;
                vnc.send_key_event(false, key)?;
            }
            return Ok(VNCEventRes::Done);
        }
        Ok(VNCEventRes::NoConnection)
    }

    fn handle_screen_takeshot(
        &mut self,
        name: String,
        span: Option<String>,
    ) -> Result<VNCEventRes, t_vnc::Error> {
        if let Some(screenshot) = self.screenshot_buffer.back() {
            if let Some(tx) = &self.screenshot_tx {
                // if has new frame, then save
                let (done_tx, done_rx) = mpsc::channel();
                if let Err(e) = tx.send(Log::Screenshot {
                    screen: screenshot.clone(),
                    name,
                    span,
                    done_tx,
                }) {
                    error!(msg = "screenshot channel closed", reason = ?e);
                    self.screenshot_tx = None;
                }
                if let Err(e) = done_rx.recv() {
                    error!(msg = "screenshot done recv failed", reason = ?e);
                    self.screenshot_tx = None;
                }
                return Ok(VNCEventRes::Done);
            }
        }
        Ok(VNCEventRes::NoConnection)
    }

    fn handle_screen_getlatest(&mut self) -> Result<VNCEventRes, t_vnc::Error> {
        if let Some(screenshot) = self.screenshot_buffer.back() {
            return Ok(VNCEventRes::Screen(screenshot.clone()));
        }
        Ok(VNCEventRes::NoConnection)
    }

    fn handle_screen_refresh(&mut self) -> Result<VNCEventRes, t_vnc::Error> {
        if let Some(vnc) = self.conn.as_mut() {
            vnc.request_update(
                Rect {
                    left: 0,
                    top: 0,
                    width: self.state.width,
                    height: self.state.height,
                },
                false,
            )?;
            return Ok(VNCEventRes::Done);
        }
        Ok(VNCEventRes::NoConnection)
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
