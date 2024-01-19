use std::{
    error::Error,
    fmt::Display,
    io,
    net::TcpStream,
    ops::Add,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{self, Duration, UNIX_EPOCH},
};

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use image::ImageBuffer;
use t_vnc::PixelFormat;
use tracing::{debug, error, info, trace, warn};

use crate::ScreenControlConsole;

use super::{data::RectContainer, pixel::RGBPixel};

pub enum VNCEventReq {
    TypeString(String),
    SendKey(String),
    MouseMove(u16, u16),
    MoveDown,
    MoveUp,
    MouseHide,
    Dump,
}

pub type PNG = ImageBuffer<image::Rgb<u8>, Vec<u8>>;

pub enum VNCEventRes {
    Done,
    Screen(PNG),
}

pub struct VNCClient {
    pub event_tx: Sender<(VNCEventReq, Sender<VNCEventRes>)>,
}

impl ScreenControlConsole for VNCClient {}

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

impl VNCClient {
    pub fn connect<A: Into<String>>(addrs: A, password: Option<String>) -> Result<Self, VNCError> {
        let stream =
            TcpStream::connect_timeout(&addrs.into().parse().unwrap(), Duration::from_secs(3))
                .map_err(VNCError::Io)?;

        let mut vnc = t_vnc::Client::from_tcp_stream(stream, false, |methods| {
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

        vnc.format();

        let size = vnc.size();
        let pixel_format = vnc.format();

        let (tx, rx) = mpsc::channel();
        let mut inner = VncClientInner {
            width: size.0,
            height: size.1,
            mouse_x: size.0,
            mouse_y: size.1,
            pixel_format,
            unstable_screen: RectContainer::new((0, 0, size.0, size.1).into()),
            stable_screen: RectContainer::new((0, 0, size.0, size.1).into()),

            event_rx: rx,
        };

        thread::spawn(move || {
            inner.pool(&mut vnc);
        });

        Ok(Self { event_tx: tx })
    }
}

struct VncClientInner {
    width: u16,
    height: u16,
    mouse_x: u16,
    mouse_y: u16,
    pixel_format: PixelFormat,
    unstable_screen: RectContainer<RGBPixel>,
    stable_screen: RectContainer<RGBPixel>,
    event_rx: Receiver<(VNCEventReq, Sender<VNCEventRes>)>,
}

impl VncClientInner {
    // vnc event loop
    fn pool(&mut self, vnc: &mut t_vnc::Client) {
        info!(msg = "start event pool loop");
        'running: loop {
            const FRAME_MS: u64 = 1000 / 60;
            let start = std::time::Instant::now();

            trace!(msg = "waiting new events");
            for event in vnc.poll_iter() {
                info!(msg = "receive new event");
                use t_vnc::client::Event;
                match event {
                    Event::Disconnected(None) => {
                        break 'running;
                    }
                    Event::Disconnected(Some(error)) => {
                        error!("server disconnected: {:?}", error);
                        break 'running;
                    }
                    Event::Resize(width, height) => self.resize_screen(width, height),
                    Event::PutPixels(rect, ref pixels) => {
                        // 获取 PixelFormat 对象和像素数据
                        let new_rect = RectContainer::new_with_data(
                            (rect.left, rect.top, rect.width, rect.height).into(),
                            convert_to_rgb(&self.pixel_format, pixels),
                        );
                        self.copy_rect(new_rect);
                    }
                    Event::CopyPixels { src, dst } => {
                        let data = self
                            .unstable_screen
                            .get_rect(src.left, src.top, src.width, src.height);
                        self.unstable_screen.update(RectContainer {
                            rect: dst.into(),
                            data,
                        });
                    }
                    Event::EndOfFrame => {
                        info!(msg = "Event::EndOfFrame");
                        self.stable_screen = self.unstable_screen.clone();
                        let image_path = format!(
                            "./.private/assets/output-{}.png",
                            time::SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        );
                        self.dump_screen().save(image_path).unwrap();
                        self.dump_screen()
                            .save("./.private/assets/output-latest.png")
                            .unwrap();
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

            //
            let mouse_buttons_mask = 0u8;
            let mouse_button_left = 0x01;
            while let Ok((msg, tx)) = self.event_rx.recv_timeout(timeout()) {
                let res = match msg {
                    VNCEventReq::TypeString(_) => unimplemented!(),
                    VNCEventReq::SendKey(_) => unimplemented!(),
                    VNCEventReq::MouseMove(x, y) => {
                        debug!(msg = "mouse move", x = x, y = y);
                        vnc.send_pointer_event(mouse_buttons_mask, x, y).unwrap();
                        self.mouse_x = x;
                        self.mouse_y = y;
                        VNCEventRes::Done
                    }
                    VNCEventReq::MoveDown => {
                        let mouse_button = mouse_buttons_mask | mouse_button_left;
                        vnc.send_pointer_event(mouse_button, self.mouse_x, self.mouse_y)
                            .unwrap();
                        VNCEventRes::Done
                    }
                    VNCEventReq::MoveUp => {
                        let mouse_button = mouse_buttons_mask & !mouse_button_left;
                        vnc.send_pointer_event(mouse_button, self.mouse_x, self.mouse_y)
                            .unwrap();
                        VNCEventRes::Done
                    }
                    VNCEventReq::Dump => {
                        let screen = self.dump_screen();
                        VNCEventRes::Screen(screen)
                    }
                    VNCEventReq::MouseHide => {
                        vnc.send_pointer_event(mouse_button_left, self.width, self.height)
                            .unwrap();
                        self.mouse_x = self.width;
                        self.mouse_y = self.height;
                        VNCEventRes::Done
                    }
                };
                tx.send(res).unwrap();
            }

            vnc.request_update((&self.unstable_screen.rect).into(), true)
                .unwrap();
        }
    }

    fn resize_screen(&mut self, width: u16, height: u16) {
        let screen_clone = RectContainer::new((0, 0, width, height).into());
        self.unstable_screen.update(screen_clone);
    }

    // update some pixels
    fn copy_rect(&mut self, rect: RectContainer<RGBPixel>) {
        self.unstable_screen.update(rect);
    }

    fn dump_screen(&self) -> PNG {
        let mut image_buffer: PNG = ImageBuffer::new(
            self.stable_screen.rect.width as u32,
            self.stable_screen.rect.height as u32,
        );

        for (i, pixel) in image_buffer.chunks_mut(3).enumerate() {
            pixel[0] = self.stable_screen.data[i][0];
            pixel[1] = self.stable_screen.data[i][1];
            pixel[2] = self.stable_screen.data[i][2];
        }

        image_buffer
    }
}

fn convert_to_rgb(pixel_format: &PixelFormat, raw_pixel_chunks: &[u8]) -> Vec<[u8; 3]> {
    let byte_per_pixel = pixel_format.bits_per_pixel as usize / 8;
    let len = raw_pixel_chunks.len() / byte_per_pixel;
    let mut image_buffer: Vec<[u8; 3]> = Vec::new();

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

        image_buffer.push([red, green, blue]);
    }

    image_buffer
}

// convert vnc pixels to png
#[allow(dead_code)]
fn convert_to_imagebuffer(
    width: u16,
    height: u16,
    pixel_format: &PixelFormat,
    raw_pixel_chunks: &[u8],
) -> PNG {
    let mut image_buffer: PNG = ImageBuffer::new(width as u32, height as u32);

    let rgb_lsit = convert_to_rgb(pixel_format, raw_pixel_chunks);
    for (i, pixel) in image_buffer.chunks_mut(3).enumerate() {
        pixel[0] = rgb_lsit[i][0];
        pixel[1] = rgb_lsit[i][1];
        pixel[2] = rgb_lsit[i][2];
    }

    image_buffer
}

#[cfg(test)]
mod test {
    use image::ImageBuffer;

    use super::PNG;

    #[test]
    pub fn test_gen_png() {
        let mut image_buffer: PNG = ImageBuffer::new(10, 10);
        for pixel in image_buffer.chunks_mut(3) {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 255;
        }
        // image_buffer.save("output-a.png");
    }
}
