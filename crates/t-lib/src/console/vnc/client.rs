use std::net::{TcpStream, ToSocketAddrs};

use anyhow::{Ok, Result};
use log::{debug, error};

use crate::console::ScreenControlConsole;

use super::{data::FullScreen, pixel::RGBAPixel};

pub struct VNCClient {
    vnc: vnc::Client,
    screen: Option<FullScreen>,
}

impl ScreenControlConsole for VNCClient {}

impl VNCClient {
    pub fn connect<A: ToSocketAddrs>(addrs: A, password: Option<String>) -> Result<Self> {
        let tcp = TcpStream::connect(addrs)?;
        let vnc = vnc::Client::from_tcp_stream(tcp, false, |methods| {
            debug!("available authentication methods: {:?}", methods);
            if let Some(method) = methods.iter().next() {
                match method {
                    vnc::client::AuthMethod::None => return Some(vnc::client::AuthChoice::None),
                    vnc::client::AuthMethod::Password => {
                        return match password {
                            None => None,
                            Some(ref password) => {
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

        let mut res = Self { vnc, screen: None };
        res.spawn_loop();
        Ok(res)
    }

    fn spawn_loop(&mut self) {}

    fn pool(&mut self) {
        self.vnc.format();
        'running: loop {
            for event in self.vnc.poll_iter() {
                use vnc::client::Event;

                match event {
                    Event::Disconnected(None) => break 'running,
                    Event::Disconnected(Some(error)) => {
                        error!("server disconnected: {:?}", error);
                        break 'running;
                    }
                    Event::Resize(_new_width, _new_height) => {
                        // width = new_width;
                        // height = new_height;
                        // renderer
                        //     .window_mut()
                        //     .unwrap()
                        //     .set_size(width as u32, height as u32);
                        // screen = renderer
                        //     .create_texture_streaming(sdl_format, (width as u32, height as u32))
                        //     .unwrap();
                        // incremental = false;
                    }
                    Event::PutPixels(_vnc_rect, ref _pixels) => {
                        // let sdl_rect = SdlRect::new_unwrap(
                        //     vnc_rect.left as i32,
                        //     vnc_rect.top as i32,
                        //     vnc_rect.width as u32,
                        //     vnc_rect.height as u32,
                        // );
                        // screen
                        //     .update(
                        //         Some(sdl_rect),
                        //         pixels,
                        //         sdl_format.byte_size_of_pixels(vnc_rect.width as usize),
                        //     )
                        //     .unwrap();
                        // renderer.copy(&screen, Some(sdl_rect), Some(sdl_rect));
                        // incremental |= vnc_rect
                        //     == vnc::Rect {
                        //         left: 0,
                        //         top: 0,
                        //         width,
                        //         height,
                        //     };
                    }
                    // Event::CopyPixels {
                    //     src: vnc_src,
                    //     dst: vnc_dst,
                    // } => {
                    //     let sdl_src = SdlRect::new_unwrap(
                    //         vnc_src.left as i32,
                    //         vnc_src.top as i32,
                    //         vnc_src.width as u32,
                    //         vnc_src.height as u32,
                    //     );
                    //     let sdl_dst = SdlRect::new_unwrap(
                    //         vnc_dst.left as i32,
                    //         vnc_dst.top as i32,
                    //         vnc_dst.width as u32,
                    //         vnc_dst.height as u32,
                    //     );
                    //     let pixels = renderer.read_pixels(Some(sdl_src), sdl_format).unwrap();
                    //     screen
                    //         .update(
                    //             Some(sdl_dst),
                    //             &pixels,
                    //             sdl_format.byte_size_of_pixels(vnc_dst.width as usize),
                    //         )
                    //         .unwrap();
                    //     renderer.copy(&screen, Some(sdl_dst), Some(sdl_dst));
                    // }
                    // Event::EndOfFrame => {
                    //     if qemu_hacks {
                    //         let network_rtt = sdl_timer.ticks() - qemu_prev_update;
                    //         // qemu_network_rtt = network_rtt;
                    //         qemu_network_rtt = qemu_network_rtt * 80 / 100 + network_rtt * 20 / 100;
                    //         qemu_prev_update = sdl_timer.ticks();
                    //         qemu_next_update = sdl_timer.ticks() + qemu_network_rtt / 2;
                    //         debug!("network RTT: {} ms", qemu_network_rtt);
                    //     }
                    // }
                    // Event::Clipboard(ref text) => {
                    // Event::SetCursor {
                    _ => unimplemented!(), /* ignore unsupported events */
                }
            }
        }
    }

    // update full pixels
    pub fn update_screen(&mut self) {
        unimplemented!()
    }

    // update some pixels
    pub fn update_rect(&mut self) {
        unimplemented!()
    }
}
