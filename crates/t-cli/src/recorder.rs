// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use self::deque::Deque;
use chrono::{DateTime, Local};
use eframe::egui::{
    self, Color32, ColorImage, Margin, Pos2, RichText, Sense, Stroke, TextureHandle, TextureOptions,
};
use egui_notify::Toast;
use image::DynamicImage;
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender},
    thread,
    time::{self, Duration, Instant, UNIX_EPOCH},
};
use t_binding::api;
use t_console::PNG;
use t_runner::needle::NeedleConfig;
use tracing::{debug, error, warn};
use tracing_core::Level;
mod deque;
mod helper;

enum RecordMode {
    Edit,
    Interact,
    View,
}

struct Screenshot {
    recv_time: DateTime<Local>,
    source: PNG,
    image: Option<TextureHandle>,
    #[allow(unused)]
    thumbnail: Option<TextureHandle>,
}

impl Screenshot {
    pub fn new(source: PNG, recv_time: DateTime<Local>) -> Self {
        Self {
            recv_time,
            source,
            image: None,
            thumbnail: None,
        }
    }

    fn clone_source(&self) -> Self {
        Self {
            recv_time: self.recv_time,
            source: self.source.clone(),
            image: None,
            thumbnail: None,
        }
    }

    fn image(&mut self, ctx: &egui::Context, use_rayon: bool) -> egui::Image {
        let sized_image = match &self.image {
            None => {
                // update screenshot
                let color_image = to_egui_rgb_color_image(&self.source, use_rayon);
                let handle = ctx.load_texture(
                    "current screenshot",
                    color_image,
                    TextureOptions {
                        ..Default::default()
                    },
                );
                let sized_image = egui::load::SizedTexture::new(handle.id(), handle.size_vec2());
                self.image = Some(handle);
                sized_image
            }
            Some(handle) => egui::load::SizedTexture::new(handle.id(), handle.size_vec2()),
        };
        egui::Image::from_texture(sized_image)
    }

    fn thumbnail(&mut self, ctx: &egui::Context, use_rayon: bool) -> egui::Image {
        if let Some(thumbnail) = self.thumbnail.as_ref() {
            let sized_image = egui::load::SizedTexture::new(thumbnail.id(), thumbnail.size_vec2());
            egui::Image::from_texture(sized_image)
        } else {
            // generate thumbnail looks too slow, so commented now
            return self.image(ctx, use_rayon);

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

struct SampleStatus {
    start: Instant,
    samply_rate: Duration,
    screenshot_count: usize,

    vnc_fps: usize,
    gui_fps: usize,
    frame_render: Duration,
    frame_renders: Vec<Duration>,
}

impl SampleStatus {
    pub fn update(&mut self) {
        self.start += self.samply_rate;
        self.vnc_fps = self.screenshot_count;

        let mut sum = Duration::ZERO;
        for frame in &self.frame_renders {
            sum += *frame;
        }
        sum /= self.frame_renders.len() as u32;
        self.frame_render = sum;

        self.gui_fps = self.frame_renders.len();

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

struct EguiFrameStatus {
    screenshot_interval: Option<Duration>,
    egui_interval: Option<Duration>,
    egui_start: Instant,
    last_screenshot: Instant,
}

impl EguiFrameStatus {}

impl Default for EguiFrameStatus {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            screenshot_interval: Some(Duration::from_secs_f32(1. / 10.)),
            egui_interval: Some(Duration::from_secs_f32(1. / 30.)),
            egui_start: now,
            last_screenshot: now,
        }
    }
}

pub struct Recorder {
    show_confirmation_dialog: bool,
    allowed_to_close: bool,
    stop_tx: Sender<()>,

    // speed
    use_rayon: bool,

    // frame evenv count
    frame_status: EguiFrameStatus,
    sample_status: SampleStatus,

    // screenshot
    mode: RecordMode,

    // screenshots
    max_screenshot_num: usize,
    #[allow(unused)]
    screenshot_rx: Option<Receiver<PNG>>,
    screenshots: std::collections::VecDeque<Screenshot>,

    // edit mode
    type_string: String,
    send_key: String,

    // interact mode
    needle_dir: PathBuf,
    needle_name: String,
    mouse_click_mode: bool,
    mouse_click_point: Option<(bool, f32, f32)>,
    drag_pos: Pos2,
    drag_rect: Option<RectF32>,
    drag_rects: Option<Vec<DragedRect>>,
    current_screenshot: Option<Screenshot>,
    needles: Vec<NeedleSource>,

    // logs
    toasts: egui_notify::Toasts,
    logs: Deque<(tracing_core::Level, String)>,
}

struct NeedleSource {
    screenshot: Screenshot,
    rects: Vec<DragedRect>,
    name: String,
}

impl NeedleSource {
    pub fn save_to_file(&self, dir: impl AsRef<Path>) -> Result<(), ()> {
        let mut path = PathBuf::new();
        path.push(dir);
        let t = time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let image_name = format!("{}-{}.png", self.name, t);
        path.push(image_name);
        self.save_png(&path)?;
        path.pop();

        let json_file = format!("{}-{}.json", self.name, t);
        path.push(json_file);
        self.save_json(&path)?;
        Ok(())
    }

    pub fn save_png(&self, p: impl AsRef<Path>) -> Result<(), ()> {
        self.screenshot.save_to_file(p.as_ref())?;
        Ok(())
    }

    pub fn save_json(&self, p: impl AsRef<Path>) -> Result<(), ()> {
        let mut areas = Vec::new();
        for DragedRect { hover: _, rect } in &self.rects {
            let area = t_runner::needle::Area {
                type_field: "match".to_string(),
                left: rect.left as u16,
                top: rect.top as u16,
                width: rect.width as u16,
                height: rect.height as u16,
            };
            areas.push(area);
        }
        let cfg = NeedleConfig {
            areas,
            properties: Vec::new(),
            tags: vec![self.name.clone()],
        };
        let s = serde_json::to_string(&cfg).map_err(|_| ())?;
        fs::write(p, s).map_err(|_| ())?;
        Ok(())
    }
}

pub struct RecorderBuilder {
    // required
    stop_tx: Sender<()>,
    screenshot_rx: Option<Receiver<PNG>>,

    // option
    max_screenshot_num: usize,
    needle_dir: Option<String>,
}

impl RecorderBuilder {
    pub fn new(stop_tx: Sender<()>) -> Self {
        Self {
            stop_tx,
            screenshot_rx: None,
            max_screenshot_num: 60,
            needle_dir: None,
        }
    }

    pub fn with_max_screenshots(mut self, num: usize) -> Self {
        self.max_screenshot_num = num;
        self
    }

    pub fn with_screenshot_rx(mut self, rx: Receiver<PNG>) -> Self {
        self.screenshot_rx = Some(rx);
        self
    }

    pub fn with_needle_dir(mut self, dir: String) -> Self {
        self.needle_dir = Some(dir);
        self
    }

    pub fn build(self) -> Recorder {
        Recorder {
            show_confirmation_dialog: false,
            allowed_to_close: false,

            use_rayon: false,

            frame_status: Default::default(),
            sample_status: Default::default(),

            stop_tx: self.stop_tx,
            screenshot_rx: self.screenshot_rx,
            mode: RecordMode::Interact,

            max_screenshot_num: self.max_screenshot_num,
            screenshots: VecDeque::new(),

            // control
            type_string: String::new(),
            send_key: String::new(),

            // edit
            current_screenshot: None,
            needle_dir: PathBuf::new(),
            needle_name: String::new(),
            mouse_click_mode: false,
            mouse_click_point: None,
            drag_pos: Pos2 { x: 0., y: 0. },
            drag_rects: None,
            drag_rect: None,
            needles: Vec::new(),

            // logs
            toasts: egui_notify::Toasts::new()
                .with_anchor(egui_notify::Anchor::BottomRight) // 10 units from the bottom right corner
                .with_margin((-10.0, -10.0).into()),
            logs: Deque::new(100),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RectF32 {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl RectF32 {
    #[allow(unused)]
    pub fn add_delta_f32_noreverse(&mut self, x: f32, y: f32) -> &mut Self {
        self.width += x;
        self.height += y;
        self
    }

    pub fn reverse_if_needed(&mut self) -> &mut Self {
        if self.width < 0. {
            let new_left = self.left + self.width;
            let new_left = if new_left < 0. { 0. } else { new_left };
            self.width = self.left - new_left;
            self.left = new_left;
        }

        if self.height < 0. {
            let new_top = self.top + self.height;
            let new_top = if new_top < 0. { 0. } else { new_top };
            self.height = self.top - new_top;
            self.top = new_top;
        }

        self
    }

    #[allow(unused)]
    fn add_delta_f32(&mut self, x: f32, y: f32) {
        let Self {
            left,
            top,
            width,
            height,
        } = self;
        Self::add_delta_f32_one_side(left, width, x);
        Self::add_delta_f32_one_side(top, height, y);
    }

    fn add_delta_f32_one_side(left: &mut f32, width: &mut f32, x: f32) {
        let l = *left;
        let mut r = l + *width;
        r += x;

        let mut new_l = l.min(r);
        let new_r = l.max(r);
        if new_l < 0. {
            new_l = 0.;
        }
        *left = new_l;
        *width = new_r - new_l;
    }

    fn add_delta_egui_rect(&self, delta: &egui::Rect) -> egui::Rect {
        egui::Rect {
            min: Pos2 {
                x: self.left + delta.left(),
                y: self.top + delta.top(),
            },
            max: Pos2 {
                x: self.left + self.width + delta.left(),
                y: self.top + self.height + delta.top(),
            },
        }
    }
}

#[test]
fn test_transform_one() {
    let mut r = RectF32 {
        left: 2.,
        top: 2.,
        width: 0.,
        height: 0.,
    };
    r.add_delta_f32(-1., -1.);
    assert_eq!(r.left, 1.);
    assert_eq!(r.top, 1.);
    assert_eq!(r.width, 1.);
    assert_eq!(r.height, 1.);

    r.add_delta_f32_noreverse(5., 5.);

    assert_eq!(r.left, 1.);
    assert_eq!(r.top, 1.);
    assert_eq!(r.width, 6.);
    assert_eq!(r.height, 6.);

    r.add_delta_f32_noreverse(-7., -7.);
    assert_eq!(r.left, 1.);
    assert_eq!(r.top, 1.);
    assert_eq!(r.width, -1.);
    assert_eq!(r.height, -1.);

    r.reverse_if_needed();
    assert_eq!(r.left, 0.);
    assert_eq!(r.top, 0.);
    assert_eq!(r.width, 1.);
    assert_eq!(r.height, 1.);
}

#[derive(Debug, Clone, Copy)]
struct DragedRect {
    pub hover: bool,
    pub rect: RectF32,
}

fn to_egui_rgb_color_image(image: &PNG, use_rayon: bool) -> ColorImage {
    // NOTE: load image too slow, use rayon speed up 3x
    let pixels = if use_rayon {
        use rayon::prelude::*;
        image
            .data
            .par_chunks_exact(3)
            .map(|p| Color32::from_rgb(p[0], p[1], p[2]))
            .collect()
    } else {
        image
            .data
            .chunks_exact(3)
            .map(|p| Color32::from_rgb(p[0], p[1], p[2]))
            .collect()
    };
    egui::ColorImage {
        size: [image.width as usize, image.height as usize],
        pixels,
    }
}

impl Recorder {
    pub fn start(self) {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_resizable(true)
                .with_inner_size([1920.0, 1080.0]),
            ..Default::default()
        };

        if let Err(e) = eframe::run_native(
            "Confirm exit",
            options,
            Box::new(|cc| {
                egui_extras::install_image_loaders(&cc.egui_ctx);
                Box::new(self)
            }),
        ) {
            error!(msg = "gui failed", reason=?e)
        }
    }
}

impl Recorder {
    fn pre_frame(&mut self, _ctx: &egui::Context) {
        self.frame_status.egui_start = Instant::now();

        // if already got new screenshot in this frame, then skip
        if let Some(screenshot_interval) = self.frame_status.screenshot_interval {
            if Instant::now() < self.frame_status.last_screenshot + screenshot_interval {
                return;
            }
        }
        if let Ok(screenshot) = api::vnc_take_screenshot() {
            // append new screenshot
            // update status
            self.frame_status.last_screenshot = Instant::now();
            self.sample_status.screenshot_count += 1;

            // handle too many
            if self.screenshots.len() == self.max_screenshot_num {
                self.screenshots.pop_front();
            }

            self.screenshots
                .push_back(Screenshot::new(screenshot, Local::now()));
        }
    }

    fn after_frame(&mut self, ctx: &egui::Context) {
        // notify
        while let Some((level, log)) = self.logs.pop_front() {
            let mut toast = Toast::custom(log, helper::tracing_level_2_toast_level(level));
            toast
                .set_duration(Some(Duration::from_secs(3)))
                .set_show_progress_bar(true);
            self.toasts.add(toast);
        }
        self.toasts.show(ctx);

        // calc render time
        let egui_elasped = Instant::now() - self.frame_status.egui_start;
        self.sample_status.frame_renders.push(egui_elasped);

        // slepp until next frame
        let elpase_sample = Instant::now() - self.sample_status.start;
        if elpase_sample > self.sample_status.samply_rate {
            self.sample_status.update();
            // update phy frame status
            debug!(
                "receive {} new screenshot in 1s",
                self.sample_status.screenshot_count
            );
        }

        if let Some(internal) = self.frame_status.egui_interval {
            let elpase = Instant::now() - self.frame_status.egui_start;
            if elpase < internal {
                thread::sleep(internal - elpase);
            }
        }

        ctx.request_repaint();
    }

    fn render_main(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::containers::Frame {
                // fill: Color32::LIGHT_BLUE,
                inner_margin: Margin {
                    left: 10.,
                    right: 10.,
                    top: 10.,
                    bottom: 10.,
                },
                stroke: Stroke::new(
                    2.,
                    match self.mode {
                        RecordMode::Edit => Color32::YELLOW,
                        RecordMode::Interact => Color32::GREEN,
                        RecordMode::View => Color32::LIGHT_BLUE,
                    },
                ),
                ..Default::default()
            })
            .show(ctx, |ui| {
                egui::ScrollArea::both().show(ui, |ui| {
                    match self.mode {
                        RecordMode::Interact => {
                            if let Some(screenshot) = self.screenshots.back_mut() {
                                // render current screenshot
                                let screenshot = ui.add(
                                    screenshot
                                        .image(ctx, self.use_rayon)
                                        .sense(Sense::click_and_drag()),
                                );

                                // if mouse move out of image, do nothing
                                if let Some(pos) = screenshot.hover_pos() {
                                    let relative_x = (pos.x as u16)
                                        .saturating_sub(screenshot.rect.left() as u16);
                                    let relative_y =
                                        (pos.y as u16).saturating_sub(screenshot.rect.top() as u16);

                                    if let Err(e) = api::vnc_mouse_move(relative_x, relative_y) {
                                        self.logs.push((
                                            Level::ERROR,
                                            format!("mouse move failed, reason = {:?}", e),
                                        ));
                                    }
                                } else {
                                    return;
                                }

                                // TODO: fix drag
                                if screenshot.drag_started() {
                                    if let Some(pos) = screenshot.interact_pointer_pos() {
                                        self.drag_pos = pos;
                                        if let Err(e) = api::vnc_mouse_keydown() {
                                            self.logs.push((
                                                Level::ERROR,
                                                format!("mouse key down failed, reason = {:?}", e),
                                            ));
                                        }
                                    }
                                } else if screenshot.dragged() {
                                    self.drag_pos += screenshot.drag_delta();
                                    if let Err(e) = api::vnc_mouse_drag(
                                        self.drag_pos.x as u16,
                                        self.drag_pos.y as u16,
                                    ) {
                                        self.logs.push((
                                            Level::ERROR,
                                            format!("mouse drag failed, reason = {:?}", e),
                                        ));
                                    }
                                } else if screenshot.drag_released() {
                                    if let Err(e) = api::vnc_mouse_keyup() {
                                        self.logs.push((
                                            Level::ERROR,
                                            format!("mouse key down failed, reason = {:?}", e),
                                        ));
                                    }
                                }

                                if screenshot.clicked() {
                                    if let Err(e) = api::vnc_mouse_click() {
                                        self.logs.push((
                                            Level::ERROR,
                                            format!("mouse click failed, reason = {:?}", e),
                                        ));
                                    }
                                }

                                if screenshot.secondary_clicked() {
                                    if let Err(e) = api::vnc_mouse_rclick() {
                                        self.logs.push((
                                            Level::ERROR,
                                            format!("mouse right click failed, reason = {:?}", e),
                                        ));
                                    }
                                }
                            }
                        }
                        RecordMode::Edit => {
                            // handle select event
                            if self.current_screenshot.is_none() {
                                if let Some(screenshot) = self.screenshots.back() {
                                    self.current_screenshot = Some(screenshot.clone_source());
                                }
                            }
                            if let Some(screenshot) = &mut self.current_screenshot {
                                let mut screenshot = ui.add(
                                    screenshot
                                        .image(ctx, self.use_rayon)
                                        .sense(Sense::click_and_drag()),
                                );

                                if let Some(pos_max) = screenshot.hover_pos() {
                                    let x = pos_max.x - screenshot.rect.left();
                                    let y = pos_max.y - screenshot.rect.top();
                                    screenshot = screenshot.on_hover_text_at_pointer(format!(
                                        "x: {:.1}, y: {:.1}",
                                        x, y
                                    ));
                                }

                                if self.mouse_click_mode {
                                    if screenshot.clicked() {
                                        if let Some(click_point) = screenshot.hover_pos() {
                                            self.toasts.info("add pos");
                                            self.mouse_click_point = Some((
                                                false,
                                                click_point.x - screenshot.rect.left(),
                                                click_point.y - screenshot.rect.left(),
                                            ));
                                        }
                                    }
                                } else {
                                    if screenshot.drag_started() && self.drag_rect.is_none() {
                                        if let Some(start_point) = screenshot.interact_pointer_pos()
                                        {
                                            let drag_rect = RectF32 {
                                                left: start_point.x - screenshot.rect.left(),
                                                top: start_point.y - screenshot.rect.top(),
                                                width: 0.,
                                                height: 0.,
                                            };
                                            self.drag_rect = Some(drag_rect);
                                        }
                                    }
                                    if screenshot.dragged() {
                                        if let Some(rect) = self.drag_rect.as_mut() {
                                            if let Some(pos_max) = screenshot.interact_pointer_pos()
                                            {
                                                rect.width =
                                                    pos_max.x - screenshot.rect.left() - rect.left;
                                                rect.height =
                                                    pos_max.y - screenshot.rect.top() - rect.top;
                                            }

                                            // let delta = screenshot.drag_delta();
                                            // rect.add_delta_f32_noreverse(delta.x, delta.y);

                                            let rect = rect
                                                .clone()
                                                .reverse_if_needed()
                                                .add_delta_egui_rect(&screenshot.rect);
                                            ui.painter().rect_filled(
                                                rect,
                                                0.0,
                                                Color32::from_rgba_premultiplied(0, 255, 0, 100),
                                            );
                                        }
                                    }
                                    if screenshot.drag_released() {
                                        if let Some(mut rect) = self.drag_rect.take() {
                                            rect.reverse_if_needed();
                                            if rect.width != 0. && rect.height != 0. {
                                                if self.drag_rects.is_none() {
                                                    self.drag_rects = Some(Vec::new());
                                                }
                                                if let Some(rects) = self.drag_rects.as_mut() {
                                                    rects.push(DragedRect { hover: false, rect });
                                                }
                                            }
                                        }
                                    }
                                }

                                // draw selected rect
                                if let Some(rects) = self.drag_rects.as_ref() {
                                    for DragedRect { hover, rect } in rects.iter() {
                                        let rect = rect.add_delta_egui_rect(&screenshot.rect);
                                        // mesh.add_colored_rect(rect, Color32::LIGHT_BLUE);
                                        ui.painter().rect_filled(
                                            rect,
                                            0.0,
                                            if *hover {
                                                Color32::from_rgba_premultiplied(255, 0, 0, 30)
                                            } else {
                                                Color32::from_rgba_premultiplied(0, 255, 0, 100)
                                            },
                                        );
                                    }
                                }

                                // draw selected rect
                                if let Some((hover, x, y)) = &self.mouse_click_point {
                                    ui.painter().circle_filled(
                                        Pos2 {
                                            x: x + screenshot.rect.left(),
                                            y: y + screenshot.rect.left(),
                                        },
                                        10.,
                                        if *hover {
                                            Color32::from_rgba_premultiplied(255, 0, 0, 30)
                                        } else {
                                            Color32::from_rgba_premultiplied(0, 0, 255, 30)
                                        },
                                    );
                                }
                            }
                        }
                        RecordMode::View => {
                            if self.current_screenshot.is_none() {
                                if let Some(screenshot) = self.screenshots.back() {
                                    self.current_screenshot = Some(screenshot.clone_source());
                                }
                            }
                            if let Some(screenshot) = &mut self.current_screenshot {
                                ui.add(
                                    screenshot
                                        .image(ctx, self.use_rayon)
                                        .sense(Sense::click_and_drag()),
                                );
                            }
                        }
                    }
                })
            });
    }

    fn render_bottom(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // screenshots
        egui::TopBottomPanel::bottom("screenshots").show(ctx, |ui| {
            // ui.set_max_height(ui.available_height() / 4.);
            ui.set_max_height(200.);
            ui.heading(format!(
                "screenshot buffer count: {}",
                self.screenshots.len()
            ));
            egui::ScrollArea::horizontal().show(ui, |ui| {
                // row of screenshots
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    let mut deleted = Vec::new();
                    for (i, screenshot) in self.screenshots.iter_mut().rev().enumerate() {
                        ui.with_layout(egui::Layout::top_down(egui::Align::TOP), |ui| {
                            ui.group(|ui| {
                                // top control bar
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::LEFT),
                                    |ui| {
                                        ui.label(format!(
                                            "{}",
                                            screenshot.recv_time.format("%H:%M:%S")
                                        ));
                                        if ui.button("del").clicked() {
                                            deleted.push(i);
                                        }
                                    },
                                );
                                // thumbnail
                                let thumbnail = ui.add(
                                    screenshot.thumbnail(ctx, self.use_rayon).max_height(200.),
                                );
                                if thumbnail.clicked() {
                                    self.mode = RecordMode::View;
                                    self.current_screenshot = Some(screenshot.clone_source());
                                }
                            });
                        });
                    }
                    let mut index: usize = self.screenshots.len();
                    self.screenshots.retain(|_| {
                        index -= 1;
                        !deleted.contains(&index)
                    });
                });
            });
        });
    }

    fn render_sidebar(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // control bar and needle list
        egui::SidePanel::right("control panel").show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_source("control panel")
                .show(ui, |ui| {
                    ui.set_height(ui.available_height() / 4.);

                    if ui
                        .button(match self.mode {
                            RecordMode::Edit => "vnc client",
                            RecordMode::Interact => "edit mode",
                            RecordMode::View => "vnc client",
                        })
                        .clicked()
                    {
                        match self.mode {
                            RecordMode::Edit => self.mode = RecordMode::Interact,
                            RecordMode::Interact => self.mode = RecordMode::Edit,
                            RecordMode::View => self.mode = RecordMode::Interact,
                        }
                    };

                    match self.mode {
                        RecordMode::Edit => {}
                        RecordMode::View => {}
                        RecordMode::Interact => {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                                if ui.button("send string").clicked()
                                    && api::vnc_type_string(self.type_string.clone()).is_err()
                                {
                                    self.logs
                                        .push((Level::ERROR, "send text failed".to_string()));
                                }
                                ui.text_edit_singleline(&mut self.type_string);
                            });
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                                if ui.button("send key").clicked()
                                    && api::vnc_send_key(self.send_key.clone()).is_err()
                                {
                                    self.logs
                                        .push((Level::ERROR, "send key failed".to_string()));
                                }
                                ui.text_edit_singleline(&mut self.send_key);
                            });
                        }
                    }
                });

            // needle list
            egui::ScrollArea::vertical()
                .id_source("needle view")
                .show(ui, |ui| {
                    if ui
                        .button(if self.mouse_click_mode {
                            "needle crate mode: mouse"
                        } else {
                            "needle crate mode: rect drag"
                        })
                        .clicked()
                    {
                        self.mouse_click_mode = !self.mouse_click_mode;
                    };

                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        // related button
                        if ui.button("folder").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.needle_dir = path;
                            }
                        }
                        // needle dir path
                        let dir = self.needle_dir.to_string_lossy();
                        ui.label(if dir.is_empty() {"No dir selected".to_string()} else {dir.to_string()});
                    });

                    ui.group(|ui| {
                        // needle name
                        ui.text_edit_singleline(&mut self.needle_name);
                        // save button
                        if ui.button("save needle").clicked() {
                            if let Some(s) = self.current_screenshot.take() {
                                if !self.needle_name.is_empty() {
                                    if let Some(rects) = self.drag_rects.take() {
                                        let needle = NeedleSource {
                                            screenshot: s.clone_source(),
                                            rects,
                                            name: self.needle_name.clone(),
                                        };
                                        if needle.save_to_file(self.needle_dir.clone()).is_ok() {
                                            self.needles.push(needle);
                                            self.mode = RecordMode::Interact;
                                        } else {
                                            self.drag_rects = Some(needle.rects);
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(rects) = self.drag_rects.as_mut() {
                            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                let mut delete_rects = Vec::new();
                                for (i, DragedRect { hover, rect }) in
                                    rects.iter_mut().rev().enumerate()
                                {
                                    ui.with_layout(
                                        egui::Layout::left_to_right(egui::Align::LEFT),
                                        |ui| {
                                            *hover = ui
                                                .group(|ui| {
                                                    ui.label(format!(
                                                        "    rect : l:{:.1?} t:{:.1?} w:{:.1?} h:{:.1?}",
                                                        rect.left,
                                                        rect.top,
                                                        rect.width,
                                                        rect.height
                                                    ));
                                                    if ui.button("delete").clicked() {
                                                        delete_rects.push(i);
                                                    };
                                                })
                                                .response
                                                .hovered();
                                        },
                                    );
                                }

                                // handle delete action
                                let mut index: usize = rects.len();
                                rects.retain(|_| {
                                    index -= 1;
                                    !delete_rects.contains(&index)
                                });
                            });
                        }

                        let mut delated = false;
                        if let Some((hover, x, y)) = &mut self.mouse_click_point {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
                                *hover = ui
                                    .group(|ui| {
                                        ui.label(format!(
                                            "    point: {{x:{:.1?}, y:{:.1?}}}",
                                            x, y
                                        ));
                                        if ui.button("delete").clicked() {
                                            delated = true;
                                        };
                                    })
                                    .response
                                    .hovered()
                            });
                        }
                        if delated {
                            self.mouse_click_point = None;
                        }
                    });

                    ui.add_space(20.);
                    ui.heading("saved needles");
                    ui.add_space(20.);

                    for NeedleSource {
                        screenshot: _,
                        rects,
                        name,
                    } in self.needles.iter_mut()
                    {
                        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                            ui.label(
                                RichText::new(format!("tag: {}", name))
                                    .text_style(egui::TextStyle::Heading),
                            );

                            let mut deleted = Vec::new();
                            for (i, DragedRect { hover, rect }) in
                                rects.iter_mut().rev().enumerate()
                            {
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::LEFT),
                                    |ui| {
                                        *hover = ui
                                            .group(|ui| {
                                                ui.label(format!(
                                            "    rect: {{l:{:.1?}, t:{:.1?}, w:{:.1?}, h:{:.1?}}}",
                                            rect.left, rect.top, rect.width, rect.height
                                        ));

                                                if ui.button("delete").clicked() {
                                                    deleted.push(i);
                                                };
                                            })
                                            .response
                                            .hovered();
                                    },
                                );
                            }
                            // handle delete action
                            let mut index: usize = rects.len();
                            rects.retain(|_| {
                                index -= 1;
                                !deleted.contains(&index)
                            });
                        });
                    }
                });
        });
    }
}

impl eframe::App for Recorder {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // receive new screenshot
        self.pre_frame(ctx);

        // render ui
        egui::TopBottomPanel::top("tool bar").show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                if ui.button("force refresh").clicked() && api::vnc_refresh().is_err() {
                    self.logs
                        .push((Level::ERROR, "force refresh failed".to_string()));
                }
                ui.heading(format!("GUI FPS: {:>2}", self.sample_status.gui_fps));
                ui.heading(format!("VNC FPS {:>2}", self.sample_status.vnc_fps));
                if ui
                    .button(format!(
                        "rayon: {}",
                        if self.use_rayon { "on" } else { "off" }
                    ))
                    .clicked()
                {
                    self.use_rayon = !self.use_rayon;
                }
                ui.heading(format!(
                    "last frame:{:>3}ms",
                    self.sample_status.frame_render.as_millis()
                ));
                ui.heading(format!(
                    "no update:{}s",
                    (Instant::now() - self.frame_status.last_screenshot).as_secs()
                ));
            });
        });

        self.render_main(ctx, frame);
        self.render_sidebar(ctx, frame);
        self.render_bottom(ctx, frame);

        // close control
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.allowed_to_close {
                // do nothing - we will close
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_confirmation_dialog = true;
            }
        }

        let size = ctx.screen_rect();
        if self.show_confirmation_dialog {
            egui::Window::new("Do you want to quit?")
                .collapsible(false)
                .resizable(false)
                .pivot(egui::Align2::CENTER_CENTER)
                .current_pos(Pos2 {
                    x: (size.min.x + size.max.x) / 2.,
                    y: (size.min.y + size.max.y) / 2.,
                })
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("No").clicked() {
                            self.show_confirmation_dialog = false;
                            self.allowed_to_close = false;
                        }

                        if ui.button("Yes").clicked() {
                            self.show_confirmation_dialog = false;
                            self.allowed_to_close = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            if self.stop_tx.send(()).is_err() {
                                error!("server stop failed")
                            }
                        }
                    });
                });
        }

        self.after_frame(ctx);
    }
}

fn _rgb_image_to_rgba_image(rgb_image: &image::RgbImage) -> image::RgbaImage {
    let (width, height) = rgb_image.dimensions();
    let mut rgba_image = image::RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let rgb_pixel = rgb_image.get_pixel(x, y);
            let rgba_pixel = image::Rgba([rgb_pixel[0], rgb_pixel[1], rgb_pixel[2], 255]);
            rgba_image.put_pixel(x, y, rgba_pixel);
        }
    }

    rgba_image
}

#[cfg(test)]
mod test {}
