use std::{
    collections::VecDeque,
    path::Path,
    sync::{mpsc::Sender, Arc},
    time::{Duration, Instant},
};

use chrono::{DateTime, Local};
use eframe::egui::{self, TextureHandle, TextureOptions};
use image::DynamicImage;
use parking_lot::RwLock;
use t_binding::api::RustApi;
use t_console::PNG;
use tracing::{error, warn};

use super::{to_egui_rgb_color_image, util::Deque, RecordMode, Tab};

pub struct Screenshot {
    pub recv_time: DateTime<Local>,
    pub source: Arc<PNG>,
    pub handle: TextureHandle,
    #[allow(unused)]
    pub thumbnail: Option<TextureHandle>,
}

impl Screenshot {
    pub fn new(
        source: Arc<PNG>,
        ctx: &egui::Context,
        use_rayon: bool,
        recv_time: DateTime<Local>,
    ) -> Self {
        // update screenshot
        let color_image = to_egui_rgb_color_image(&source, use_rayon);
        let handle = ctx.load_texture(
            "current screenshot",
            color_image,
            TextureOptions {
                ..Default::default()
            },
        );
        Self {
            recv_time,
            source,
            handle,
            thumbnail: None,
        }
    }

    pub fn update(&mut self, source: Arc<PNG>) {
        let color_image = to_egui_rgb_color_image(&source, false);
        self.handle.set(color_image, TextureOptions::NEAREST);
        self.source = source;
    }

    pub fn clone(&self) -> Self {
        Self {
            recv_time: self.recv_time,
            source: self.source.clone(),
            handle: self.handle.clone(),
            thumbnail: None,
        }
    }

    pub fn clone_new_handle(&self, ctx: &egui::Context, use_rayon: bool) -> Self {
        // update screenshot
        let color_image = to_egui_rgb_color_image(&self.source, use_rayon);
        let handle = ctx.load_texture(
            "current screenshot",
            color_image,
            TextureOptions {
                ..Default::default()
            },
        );
        Self {
            recv_time: self.recv_time,
            source: self.source.clone(),
            handle,
            thumbnail: None,
        }
    }

    pub fn image(&self) -> egui::Image {
        egui::Image::from_texture(egui::load::SizedTexture::new(
            self.handle.id(),
            self.handle.size_vec2(),
        ))
    }

    pub fn thumbnail(&self) -> egui::Image {
        if let Some(thumbnail) = self.thumbnail.as_ref() {
            let sized_image = egui::load::SizedTexture::new(thumbnail.id(), thumbnail.size_vec2());
            egui::Image::from_texture(sized_image)
        } else {
            // generate thumbnail looks too slow, so commented now
            return self.image();

            // let default_shrink_scale = 200. / self.source.height as f32;
            // let src = &self.source;
            // let image =
            //     RgbImage::from_raw(src.width as u32, src.height as u32, src.data.clone()).unwrap();
            // let scaled_image = image::imageops::resize(
            //     &image,
            //     (src.width as f32 * default_shrink_scale) as u32,
            //     (src.height as f32 * default_shrink_scale) as u32,
            //     image::imageops::FilterType::Nearest,
            // );
            // let color_image = egui::ColorImage::from_rgb(
            //     [
            //         scaled_image.width() as usize,
            //         scaled_image.height() as usize,
            //     ],
            //     &scaled_image.as_raw(),
            // );
            // let handle = ctx.load_texture(
            //     "current screenshot",
            //     color_image,
            //     TextureOptions {
            //         ..Default::default()
            //     },
            // );
            // let sized_image = egui::load::SizedTexture::new(handle.id(), handle.size_vec2());
            // self.thumbnail = Some(handle);
            // egui::Image::from_texture(sized_image)
        }
    }

    pub fn save_to_file(&self, p: impl AsRef<Path>) -> Result<(), ()> {
        let s = &self.source;
        DynamicImage::ImageRgb8(
            image::RgbImage::from_vec(s.width as u32, s.height as u32, s.data.clone()).unwrap(),
        )
        .save(p.as_ref())
        .map_err(|e| {
            warn!(msg = "save image failed", reason=?e);
        })?;
        Ok(())
    }
}

pub struct SampleStatus {
    pub start: Instant,
    pub samply_rate: Duration,
    pub screenshot_count: usize,

    pub vnc_fps: usize,
    pub gui_fps: usize,
    pub frame_render: Duration,
    pub frame_renders: Vec<Duration>,
}

impl SampleStatus {
    pub fn update(&mut self) {
        // update every second
        self.start += self.samply_rate;
        // vnc fps = screenshot received times in this second
        self.vnc_fps = self.screenshot_count;

        // gui fps = egui frame render times in this second
        self.gui_fps = self.frame_renders.len();

        // frame render is the mean of all frame render times in this second
        let mut sum = Duration::ZERO;
        for frame in &self.frame_renders {
            sum += *frame;
        }
        let mean = sum / self.frame_renders.len() as u32;
        self.frame_render = mean;

        // refresh to zero
        self.screenshot_count = 0;
        self.frame_renders.clear();
    }
}

impl Default for SampleStatus {
    fn default() -> Self {
        Self {
            samply_rate: Duration::from_secs(1),
            start: Instant::now(),
            screenshot_count: 0,
            frame_render: Duration::ZERO,
            vnc_fps: 0,
            gui_fps: 0,
            frame_renders: Vec::new(),
        }
    }
}

pub struct EguiFrameStatus {
    pub screenshot_interval: Option<Duration>,
    pub egui_interval: Option<Duration>,
    pub egui_start: Instant,
    pub last_screenshot: Instant,
}

impl EguiFrameStatus {}

impl Default for EguiFrameStatus {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            screenshot_interval: Some(Duration::from_secs_f32(1. / 10.)),
            egui_interval: Some(Duration::from_secs_f32(1. / 60.)),
            egui_start: now,
            last_screenshot: now,
        }
    }
}

pub struct PanelState {
    pub driver: Option<(RustApi, Sender<Sender<()>>)>,

    #[allow(unused)]
    pub screenshots: RwLock<VecDeque<Screenshot>>,
    // logs
    pub logs_toasts: Deque<(tracing_core::Level, String)>,
    pub logs_history: Deque<(tracing_core::Level, String)>,
    // panel control
    pub mode: RecordMode,
    pub tab: Tab,
    // config
    pub config: Option<t_config::Config>,
    pub config_str: String,
    pub code_str: String,
    // use in editor
    pub current_screenshot: Option<Screenshot>,
}

impl PanelState {
    pub fn new(config: Option<String>) -> Self {
        let default_config_str = config.unwrap_or(
            r#"log_dir = "./logs"

        # [serial]
        # serial_file = "/dev/ttyUSB0"
        # bund_rate   = 115200

        # [ssh]
        # host        = "127.0.0.1"
        # port        = 22
        # username    = "root"
        # one of password or private_key
        # if both are set, private_key will be used first
        # if none, ~/.ssh/id_rsa will be used
        # password    = ""
        # private_key = ""

        # [vnc]
        # host = "127.0.0.1"
        # port = 5901
        # password = "123456" # optional
        # needle_dir = "./needles" # optional
                "#
            .to_string(),
        );
        Self {
            driver: None,
            screenshots: RwLock::new(VecDeque::new()),

            mode: RecordMode::Interact,
            tab: Tab::Vnc,
            logs_toasts: Deque::new(50),
            logs_history: Deque::new(1000),

            config: t_config::Config::from_toml_str(default_config_str.as_str()).ok(),
            config_str: default_config_str,
            code_str: r#"
export function prehook() {
// TODO:
}

// entry point
export function main() {
// TODO:
writeln("ls")
}

export function afterhook() {
// TODO:
}
"#
            .to_string(),
            current_screenshot: None,
        }
    }

    pub fn stop(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        let Some((_, stop_tx)) = self.driver.as_ref() else {
            return;
        };
        if stop_tx.send(tx).is_err() {
            error!("server stop failed")
        }
        if let Err(_e) = rx.recv() {
            error!("server stop failed")
        }
        self.driver = None;
    }
}
