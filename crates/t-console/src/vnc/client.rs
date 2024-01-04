use std::{
    net::TcpStream,
    time::{self, Duration, UNIX_EPOCH},
};

use anyhow::{Ok, Result};
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use image::ImageBuffer;
use tracing::{error, info};
use vnc::PixelFormat;

use crate::ScreenControlConsole;

use super::{data::RectContainer, pixel::RGBPixel};

pub struct VNCClient {
    vnc: Option<vnc::Client>,
    pixel_format: PixelFormat,
    unstable_screen: RectContainer<RGBPixel>,
    stable_screen: RectContainer<RGBPixel>,
}

impl ScreenControlConsole for VNCClient {}

impl VNCClient {
    pub fn connect<A: Into<String>>(addrs: A, password: Option<String>) -> Result<Self> {
        let stream =
            TcpStream::connect_timeout(&addrs.into().parse().unwrap(), Duration::from_secs(3))?;

        let vnc = vnc::Client::from_tcp_stream(stream, false, |methods| {
            info!("available authentication methods: {:?}", methods);
            for method in methods {
                match method {
                    vnc::client::AuthMethod::None => return Some(vnc::client::AuthChoice::None),
                    vnc::client::AuthMethod::Password => {
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
                                Some(vnc::client::AuthChoice::Password(key))
                            }
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            None
        })?;

        vnc.format();

        let size = vnc.size();
        let pixel_format = vnc.format();

        let res = Self {
            vnc: Some(vnc),
            pixel_format,
            unstable_screen: RectContainer::new(0, 0, size.0, size.1),
            stable_screen: RectContainer::new(0, 0, size.0, size.1),
        };
        Ok(res)
    }

    pub fn block_on(&mut self) {
        let vnc = self.vnc.take();
        self.pool(&mut vnc.unwrap());
    }

    // vnc event loop
    fn pool(&mut self, vnc: &mut vnc::Client) {
        info!("start loop");
        'running: loop {
            for event in vnc.poll_iter() {
                use vnc::client::Event;
                match event {
                    Event::Disconnected(None) => {
                        info!("Event::Disconnected");
                        break 'running;
                    }
                    Event::Disconnected(Some(error)) => {
                        error!("server disconnected: {:?}", error);
                        break 'running;
                    }
                    Event::Resize(width, height) => {
                        info!("Event::Resize");
                        self.resize_screen(width, height)
                    }
                    Event::PutPixels(rect, ref pixels) => {
                        info!("Event::PutPixels");
                        // 获取 PixelFormat 对象和像素数据
                        let image_buffer = convert_to_screenrect(
                            rect.left,
                            rect.top,
                            rect.width,
                            rect.height,
                            &self.pixel_format,
                            &pixels,
                        );

                        self.copy_rect(image_buffer);
                    }
                    Event::CopyPixels { src, dst } => {
                        info!("Event::CopyPixels");
                        let data = self
                            .unstable_screen
                            .copy(src.left, src.top, src.width, src.height);
                        self.unstable_screen.update(RectContainer {
                            left: dst.left,
                            top: dst.top,
                            width: dst.width,
                            height: dst.height,
                            data,
                        });
                    }
                    Event::EndOfFrame => {
                        info!("Event::EndOfFrame");
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
                    Event::Clipboard(ref _text) => {
                        info!("Event::Clipboard");
                    }
                    Event::SetCursor { .. } => {
                        info!("Event::SetCursor");
                    }
                    _ => unimplemented!(), /* ignore unsupported events */
                }
            }

            vnc.request_update(
                vnc::Rect {
                    left: 0,
                    top: 0,
                    width: self.unstable_screen.width,
                    height: self.unstable_screen.height,
                },
                true,
            )
            .unwrap();
        }
    }

    fn resize_screen(&mut self, width: u16, height: u16) {
        let screen_clone = RectContainer::new(0, 0, width, height);
        self.unstable_screen.update(screen_clone);
    }

    // update some pixels
    fn copy_rect(&mut self, rect: RectContainer<RGBPixel>) {
        self.unstable_screen.update(rect);
    }

    fn dump_screen(&self) -> ImageBuffer<image::Rgb<u8>, Vec<u8>> {
        let mut image_buffer: ImageBuffer<image::Rgb<u8>, Vec<u8>> = ImageBuffer::new(
            self.unstable_screen.width as u32,
            self.unstable_screen.height as u32,
        );

        for (i, pixel) in image_buffer.chunks_mut(3).enumerate() {
            pixel[0] = self.unstable_screen.data[i][0];
            pixel[1] = self.unstable_screen.data[i][1];
            pixel[2] = self.unstable_screen.data[i][2];
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
fn convert_to_imagebuffer(
    width: u16,
    height: u16,
    pixel_format: &PixelFormat,
    raw_pixel_chunks: &[u8],
) -> ImageBuffer<image::Rgb<u8>, Vec<u8>> {
    let mut image_buffer: ImageBuffer<image::Rgb<u8>, Vec<u8>> =
        ImageBuffer::new(width as u32, height as u32);

    let rgb_lsit = convert_to_rgb(pixel_format, raw_pixel_chunks);
    for (i, pixel) in image_buffer.chunks_mut(3).enumerate() {
        pixel[0] = rgb_lsit[i][0];
        pixel[1] = rgb_lsit[i][1];
        pixel[2] = rgb_lsit[i][2];
    }

    image_buffer
}

// convert vnc pixels to Vector
fn convert_to_screenrect(
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    pixel_format: &PixelFormat,
    raw_pixel_chunks: &[u8],
) -> RectContainer<RGBPixel> {
    let mut image_buffer: RectContainer<[u8; 3]> = RectContainer::new(left, top, width, height);
    image_buffer.data = convert_to_rgb(pixel_format, raw_pixel_chunks);
    image_buffer
}

#[cfg(test)]
mod test {
    use image::ImageBuffer;

    #[test]
    pub fn test_gen_png() {
        let mut image_buffer: ImageBuffer<image::Rgb<u8>, Vec<u8>> = ImageBuffer::new(10, 10);
        for pixel in image_buffer.chunks_mut(3) {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 255;
        }
        // image_buffer.save("output-a.png");
    }

    fn test() {
        // let image_path = format!(
        //     "./.private/assets/output-{}-{}-{}-{}-{}.png",
        //     time::SystemTime::now()
        //         .duration_since(UNIX_EPOCH)
        //         .unwrap()
        //         .as_secs(),
        //     rect.left,
        //     rect.top,
        //     rect.width,
        //     rect.height
        // );

        // // 创建 PNG 文件并保存图像缓冲区
        // debug!("{}", {
        //     image_buffer.save(image_path).unwrap();
        //     image_buffer
        //         .save("./.private/assets/output-latest.png")
        //         .unwrap();
        //     ""
        // });
    }
}
