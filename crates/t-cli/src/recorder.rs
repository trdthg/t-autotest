// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use self::deque::Deque;
use chrono::{DateTime, Local};
use eframe::egui::{
    self,
    ahash::{HashMap, HashMapExt},
    text::CursorRange,
    Color32, Margin, Pos2, Rect, RichText, Sense, TextEdit, TextureHandle, TextureOptions, Vec2,
    Widget,
};
use egui_notify::Toast;
use helper::*;
use image::DynamicImage;
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use t_binding::api::{Api, ApiTx, RustApi};
use t_console::PNG;
use t_runner::needle::NeedleConfig;
use tracing::{debug, error, info, warn};
use tracing_core::Level;
mod deque;
mod helper;

#[derive(Debug, PartialEq)]
enum RecordMode {
    Edit,
    Interact,
    View,
}

#[derive(Debug, PartialEq)]
enum Tab {
    Vnc,
    Serial,
    Ssh,
}

struct Screenshot {
    recv_time: DateTime<Local>,
    source: Arc<PNG>,
    handle: TextureHandle,
    #[allow(unused)]
    thumbnail: Option<TextureHandle>,
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

    fn clone(&self) -> Self {
        Self {
            recv_time: self.recv_time,
            source: self.source.clone(),
            handle: self.handle.clone(),
            thumbnail: None,
        }
    }

    fn image(&self) -> egui::Image {
        egui::Image::from_texture(egui::load::SizedTexture::new(
            self.handle.id(),
            self.handle.size_vec2(),
        ))
    }

    #[allow(unused)]
    fn thumbnail(&self) -> egui::Image {
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
            egui_interval: Some(Duration::from_secs_f32(1. / 60.)),
            egui_start: now,
            last_screenshot: now,
        }
    }
}

pub struct FileWatcher {
    cache: Arc<parking_lot::RwLock<HashMap<PathBuf, String>>>,
    watchers: parking_lot::Mutex<Vec<notify::RecommendedWatcher>>,
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            watchers: parking_lot::Mutex::new(Vec::new()),
        }
    }

    pub fn try_watch(&self, path: impl AsRef<Path>) {
        let path = path.as_ref().to_path_buf();
        // let path_clone = path.as_ref().to_path_buf();
        let cache = self.cache.clone();
        if cache.read().get(path.as_path()).is_none() {
            if let Ok(file) = fs::read_to_string(path.as_path()) {
                let mut lock = cache.write();
                // double check
                if lock.get(path.as_path()).is_some() {
                    return;
                }
                lock.insert(path.clone(), file);
                // lock.insert(path.clone(), file.lines().map(|s| s.to_string()).collect());
                drop(lock);

                // spawn watcher
                use notify::Watcher;
                let path_clone = path.clone();
                let mut watcher = notify::recommended_watcher(
                    move |res: Result<notify::Event, notify::Error>| match res {
                        Ok(_event) => {
                            let content = fs::read_to_string(&path_clone).unwrap_or_default();
                            let stripped = console::strip_ansi_codes(&content);
                            cache.write().insert(
                                path_clone.clone(),
                                // stripped.lines().map(|s| s.to_string()).collect(),
                                stripped.to_string(),
                            );
                        }
                        Err(e) => {
                            info!("watch error: {:?}", e);
                        }
                    },
                )
                .unwrap();
                let cfg = notify::Config::default();
                cfg.with_poll_interval(Duration::from_secs(1));
                watcher.configure(cfg).unwrap();

                let pathname = path.as_path().display();
                info!(msg = "watcher started", path = ?pathname);
                watcher
                    .watch(path.as_path(), notify::RecursiveMode::NonRecursive)
                    .unwrap();
                self.watchers.lock().push(watcher);
            }
        }
    }
}

pub struct Recorder {
    api: RustApi,
    show_confirmation_dialog: bool,
    allowed_to_close: bool,
    stop_tx: Sender<()>,

    // speed
    use_rayon: bool,

    // frame evenv count
    frame_status: Arc<parking_lot::RwLock<EguiFrameStatus>>,
    sample_status: Arc<parking_lot::RwLock<SampleStatus>>,

    // file
    file_watcher: FileWatcher,

    // screenshot
    mode: RecordMode,
    tab: Tab,
    show_config_edit_window: bool,
    config: Option<t_config::Config>,
    config_str: String,
    code_str: String,
    code_receiver: Option<Receiver<Result<(), String>>>,
    cursor_range: Option<CursorRange>,

    // screenshots
    max_screenshot_num: usize,
    #[allow(unused)]
    screenshot_rx: Option<Receiver<PNG>>,
    screenshots: Arc<parking_lot::RwLock<std::collections::VecDeque<Screenshot>>>,

    // interact mode
    needle_name: String,
    minimal_move_interval: Duration,
    last_move_interval: Instant,
    drag_pos: Pos2,
    drag_rect: Option<RectF32>,
    drag_rects: Option<Vec<DragedRect>>,
    current_screenshot: Option<Screenshot>,
    needles: Vec<NeedleSource>,

    // logs
    toasts: egui_notify::Toasts,
    logs_toasts: Deque<(tracing_core::Level, String)>,
    logs_history: Deque<(tracing_core::Level, String)>,
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
        let image_name = format!("{}.png", self.name);
        path.push(image_name);
        self.save_png(&path)?;
        path.pop();

        let json_file = format!("{}.json", self.name);
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
        for DragedRect { rect, click, .. } in &self.rects {
            let area = t_runner::needle::Area {
                type_field: "match".to_string(),
                left: rect.left as u16,
                top: rect.top as u16,
                width: rect.width as u16,
                height: rect.height as u16,
                click: click.map(|(x, y)| t_runner::needle::AreaClick {
                    left: x as u16,
                    top: y as u16,
                }),
            };
            areas.push(area);
        }
        let cfg = NeedleConfig {
            areas,
            properties: Vec::new(),
            tags: vec![self.name.clone()],
        };
        let s = serde_json::to_string_pretty(&cfg).map_err(|_| ())?;
        fs::write(p, s).map_err(|_| ())?;
        Ok(())
    }
}

pub struct RecorderBuilder {
    // required
    stop_tx: Sender<()>,
    screenshot_rx: Option<Receiver<PNG>>,
    api: ApiTx,

    // option
    max_screenshot_num: usize,
    config_str: Option<String>,
}

impl RecorderBuilder {
    pub fn new(stop_tx: Sender<()>, api: ApiTx, config_str: Option<String>) -> Self {
        Self {
            stop_tx,
            screenshot_rx: None,
            api,
            max_screenshot_num: 10,
            config_str,
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

    pub fn build(self) -> Recorder {
        Recorder {
            api: RustApi::new(self.api),
            show_confirmation_dialog: false,
            allowed_to_close: false,

            // only used in PNG to egui::ColorImage, take more cpu usage
            use_rayon: false,

            frame_status: Default::default(),
            sample_status: Default::default(),

            stop_tx: self.stop_tx,
            screenshot_rx: self.screenshot_rx,
            mode: RecordMode::Interact,
            tab: Tab::Vnc,
            show_config_edit_window: true,
            config_str: self.config_str.clone().unwrap_or(
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
            ),
            config: Some(
                t_config::Config::from_toml_str(self.config_str.unwrap().as_ref()).unwrap(),
            ),
            code_receiver: None,
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
            cursor_range: None,

            // file
            file_watcher: FileWatcher::new(),

            // screenshots buffer
            max_screenshot_num: self.max_screenshot_num,
            screenshots: Arc::new(parking_lot::RwLock::new(std::collections::VecDeque::new())),

            // edit
            current_screenshot: None,
            needle_name: String::new(),
            last_move_interval: Instant::now(),
            minimal_move_interval: Duration::from_millis(50),
            drag_pos: Pos2 { x: 0., y: 0. },
            drag_rects: None,
            drag_rect: None,
            needles: Vec::new(),

            // logs
            toasts: egui_notify::Toasts::new()
                .with_anchor(egui_notify::Anchor::BottomRight) // 10 units from the bottom right corner
                .with_margin((-10.0, -10.0).into()),
            logs_toasts: Deque::new(50),
            logs_history: Deque::new(1000),
        }
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

                let ctx = cc.egui_ctx.clone();
                let screenshots = self.screenshots.clone();
                let frame_status = self.frame_status.clone();
                let sample_status = self.sample_status.clone();
                let api = self.api.clone();
                thread::spawn(move || {
                    let interval = frame_status.read().screenshot_interval;
                    loop {
                        // if already got new screenshot in this egui frame, then skip
                        if let Some(screenshot_interval) = interval {
                            if Instant::now()
                                < frame_status.read().last_screenshot + screenshot_interval
                            {
                                continue;
                            }
                        }

                        if let Ok(screenshot) = api.vnc_get_screenshot() {
                            // update status
                            frame_status.write().last_screenshot = Instant::now();
                            sample_status.write().screenshot_count += 1;

                            // handle too many
                            if screenshots.read().len() == self.max_screenshot_num {
                                screenshots.write().pop_front();
                            }

                            // append new screenshot
                            let s = Screenshot::new(screenshot, &ctx, false, Local::now());
                            screenshots.write().push_back(s);
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                });
                Box::new(self)
            }),
        ) {
            error!(msg = "gui failed", reason=?e)
        }
    }
}

impl Recorder {
    fn pre_frame(&mut self) {
        self.frame_status.write().egui_start = Instant::now();
    }

    fn after_frame(&mut self, ctx: &egui::Context) {
        // handle notify
        while let Some((level, log)) = self.logs_toasts.pop_front() {
            let mut toast = Toast::custom(&log, helper::tracing_level_2_toast_level(level));
            toast
                .set_duration(Some(Duration::from_secs(3)))
                .set_show_progress_bar(true);
            self.toasts.add(toast);
            self.logs_history.push_back((level, log));
        }
        self.toasts.show(ctx);

        let mut sample_status = self.sample_status.write();
        let frame_status = self.frame_status.read();

        // calc render time
        let egui_elasped = Instant::now() - frame_status.egui_start;
        sample_status.frame_renders.push(egui_elasped);

        // sleep until next frame
        let elpase_sample = Instant::now() - sample_status.start;
        if elpase_sample > sample_status.samply_rate {
            sample_status.update();
            // update phy frame status
            debug!(
                "receive {} new screenshot in 1s",
                sample_status.screenshot_count
            );
        }

        if let Some(internal) = frame_status.egui_interval {
            let elpase = Instant::now() - frame_status.egui_start;
            if elpase < internal {
                thread::sleep(internal - elpase);
            }
        }

        ctx.request_repaint();
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("force refresh").clicked() && self.api.vnc_refresh().is_err() {
                self.logs_toasts
                    .push((Level::ERROR, "force refresh failed".to_string()));
            }
            let sample_status = self.sample_status.read();
            ui.colored_label(
                Color32::RED,
                RichText::new(format!(
                    "GUI FPS: {:>2}, {:>3}ms",
                    sample_status.gui_fps,
                    sample_status.frame_render.as_millis()
                ))
                .heading(),
            );

            ui.colored_label(
                Color32::YELLOW,
                RichText::new(format!("VNC FPS {:>2}", sample_status.vnc_fps)).heading(),
            );
            drop(sample_status);

            if ui
                .button(format!(
                    "rayon: {}",
                    if self.use_rayon { "on" } else { "off" }
                ))
                .clicked()
            {
                self.use_rayon = !self.use_rayon;
            }

            ui.colored_label(
                Color32::GREEN,
                RichText::new(format!(
                    "vnc no update:{}s",
                    (Instant::now() - self.frame_status.read().last_screenshot).as_secs()
                ))
                .heading(),
            );
        });
    }

    fn render_vnc(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both()
            .auto_shrink(false)
            .show_viewport(ui, |ui, _rect| {
                match self.mode {
                    RecordMode::Interact => {
                        let lock = self.screenshots.read();
                        let Some(screenshot) = lock.back() else {
                            return;
                        };

                        // render current screenshot
                        let img = screenshot.image();
                        let screenshot = ui.add(img.sense(Sense::click_and_drag()));

                        // if mouse move out of image, do nothing
                        if let Some(pos) = screenshot.hover_pos() {
                            let relative_x =
                                (pos.x as u16).saturating_sub(screenshot.rect.left() as u16);
                            let relative_y =
                                (pos.y as u16).saturating_sub(screenshot.rect.top() as u16);

                            if Instant::now() - self.last_move_interval > self.minimal_move_interval
                            {
                                if self.api.vnc_mouse_move(relative_x, relative_y).is_err() {
                                    // FIXME: too many error log
                                    // self.logs_toasts.push((
                                    //     Level::ERROR,
                                    //     format!("mouse move failed, reason = {:?}", e),
                                    // ));
                                }
                                self.last_move_interval = Instant::now();
                            }
                        }

                        // handle drag
                        if screenshot.drag_started() {
                            if let Some(pos) = screenshot.interact_pointer_pos() {
                                self.drag_pos = pos;
                                if let Err(e) = self.api.vnc_mouse_keydown() {
                                    self.logs_toasts.push((
                                        Level::ERROR,
                                        format!("mouse key down failed, reason = {:?}", e),
                                    ));
                                }
                            }
                        } else if screenshot.dragged() {
                            self.drag_pos += screenshot.drag_delta();
                            if let Err(e) = self
                                .api
                                .vnc_mouse_drag(self.drag_pos.x as u16, self.drag_pos.y as u16)
                            {
                                self.logs_toasts.push((
                                    Level::ERROR,
                                    format!("mouse drag failed, reason = {:?}", e),
                                ));
                            }
                        } else if screenshot.drag_stopped() {
                            if let Err(e) = self.api.vnc_mouse_keyup() {
                                self.logs_toasts.push((
                                    Level::ERROR,
                                    format!("mouse key down failed, reason = {:?}", e),
                                ));
                            }
                        }

                        if screenshot.clicked() {
                            if let Err(e) = self.api.vnc_mouse_click() {
                                self.logs_toasts.push((
                                    Level::ERROR,
                                    format!("mouse click failed, reason = {:?}", e),
                                ));
                            }
                        }

                        if screenshot.secondary_clicked() {
                            if let Err(e) = self.api.vnc_mouse_rclick() {
                                self.logs_toasts.push((
                                    Level::ERROR,
                                    format!("mouse right click failed, reason = {:?}", e),
                                ));
                            }
                        }
                    }
                    RecordMode::Edit => {
                        // handle screenshot
                        if let Some(screenshot) = self.screenshots.read().back() {
                            // ---------------------------------------------------------------------------------------------------------

                            let mut screenshot =
                                ui.add(screenshot.image().sense(Sense::click_and_drag()));

                            if let Some(pos_max) = screenshot.hover_pos() {
                                let x = pos_max.x - screenshot.rect.left();
                                let y = pos_max.y - screenshot.rect.top();
                                screenshot = screenshot
                                    .on_hover_text_at_pointer(format!("x: {:.1}, y: {:.1}", x, y));
                            }

                            // ---------------------------------------------------------------------------------------------------------

                            // handle rect drag
                            if screenshot.drag_started() && self.drag_rect.is_none() {
                                if let Some(start_point) = screenshot.interact_pointer_pos() {
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
                                    if let Some(pos_max) = screenshot.interact_pointer_pos() {
                                        rect.width = pos_max.x - screenshot.rect.left() - rect.left;
                                        rect.height = pos_max.y - screenshot.rect.top() - rect.top;
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
                            if screenshot.drag_stopped() {
                                if let Some(mut rect) = self.drag_rect.take() {
                                    rect.reverse_if_needed();
                                    if rect.width != 0. && rect.height != 0. {
                                        if self.drag_rects.is_none() {
                                            self.drag_rects = Some(Vec::new());
                                        }
                                        if let Some(rects) = self.drag_rects.as_mut() {
                                            rects.push(DragedRect {
                                                hover: false,
                                                rect,
                                                click: None,
                                            });
                                        }
                                    }
                                }
                            }

                            // ---------------------------------------------------------------------------------------------------------

                            // handle rects
                            if let Some(rects) = self.drag_rects.as_mut() {
                                for DragedRect { hover, rect, click } in rects.iter_mut() {
                                    // draw rect
                                    let draw_rect = rect.add_delta_egui_rect(&screenshot.rect);
                                    let rect_res =
                                        ui.allocate_rect(draw_rect, Sense::click_and_drag());
                                    ui.painter().rect_filled(
                                        draw_rect,
                                        0.0,
                                        if *hover {
                                            Color32::from_rgba_premultiplied(120, 0, 0, 30)
                                        } else {
                                            Color32::from_rgba_premultiplied(0, 120, 0, 30)
                                        },
                                    );

                                    // draw click point
                                    if let Some((x, y)) = click {
                                        let point = ui.add(|ui: &mut egui::Ui| {
                                            let circle_pos = Pos2 {
                                                x: *x + rect_res.rect.left(),
                                                y: *y + rect_res.rect.top(),
                                            };
                                            let radius = 10.;
                                            let response = ui.allocate_rect(
                                                Rect {
                                                    min: circle_pos - Vec2::splat(radius),
                                                    max: circle_pos + Vec2::splat(radius),
                                                },
                                                Sense::drag(),
                                            );
                                            ui.painter().circle_filled(
                                                response.rect.center(),
                                                radius,
                                                if *hover {
                                                    Color32::from_rgba_premultiplied(
                                                        255, 255, 255, 120,
                                                    )
                                                } else {
                                                    Color32::from_rgba_premultiplied(
                                                        255, 255, 255, 30,
                                                    )
                                                },
                                            );
                                            response
                                        });
                                        if point.dragged() {
                                            *x += point.drag_delta().x;
                                            *y += point.drag_delta().y;
                                        }
                                    }

                                    // draw resize drag button
                                    let resize_button = ui.add(|ui: &mut egui::Ui| {
                                        let circle_pos = rect_res.rect.max;
                                        let radius = 10.;
                                        let response = ui.allocate_rect(
                                            Rect {
                                                min: circle_pos - Vec2::splat(radius),
                                                max: circle_pos + Vec2::splat(radius),
                                            },
                                            Sense::drag(),
                                        );
                                        ui.painter().circle_filled(
                                            response.rect.center(),
                                            radius,
                                            Color32::from_rgba_premultiplied(255, 255, 255, 30),
                                        );
                                        response
                                    });

                                    // handle add click point
                                    if rect_res.double_clicked() {
                                        if let Some(click_point) = rect_res.interact_pointer_pos() {
                                            self.toasts.info("add pos");
                                            *click = Some((
                                                click_point.x - rect_res.rect.left(),
                                                click_point.y - rect_res.rect.top(),
                                            ));
                                        }
                                    }
                                    // handle rect drag
                                    if rect_res.dragged() {
                                        rect.left += rect_res.drag_delta().x;
                                        rect.top += rect_res.drag_delta().y;
                                    }

                                    // handle rect resize
                                    if resize_button.hover_pos().is_some() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                                    } else {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                                    }
                                    if resize_button.dragged() {
                                        rect.width += resize_button.drag_delta().x;
                                        rect.height += resize_button.drag_delta().y;
                                    }
                                }
                            }
                        }
                    }
                    RecordMode::View => {
                        let lock = self.screenshots.read();
                        let Some(screenshot) = lock.back() else {
                            return;
                        };
                        let img = screenshot.image();
                        ui.add(img);
                    }
                }
            });
    }

    fn render_logs(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            for (level, log) in self.logs_history.iter().rev() {
                let color = tracing_level_2_egui_color32(level);
                ui.colored_label(color, log);
            }
        });
    }

    #[allow(unused)]
    fn render_screenshorts(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!(
            "screenshot buffer count: {}",
            self.screenshots.read().len()
        ));
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut deleted = Vec::new();
            for (i, screenshot) in self.screenshots.read().iter().rev().enumerate() {
                ui.group(|ui| {
                    // top control bar
                    ui.horizontal(|ui| {
                        ui.label(format!("{}", screenshot.recv_time.format("%H:%M:%S")));
                        if ui.button("del").clicked() {
                            deleted.push(i);
                        }
                    });
                    // thumbnail
                    let thumbnail = ui.add(screenshot.thumbnail().max_height(200.));
                    if thumbnail.clicked() {
                        self.mode = RecordMode::View;
                        self.current_screenshot = Some(screenshot.clone());
                    }
                });
                ui.separator();
            }
            let mut index: usize = self.screenshots.read().len();
            self.screenshots.write().retain(|_| {
                index -= 1;
                !deleted.contains(&index)
            });
        });
    }

    fn render_code_editor(&mut self, ui: &mut egui::Ui) {
        // code editor
        ui.label(format!(
            "selected: {:?}",
            self.cursor_range.map(|r| r.as_sorted_char_range())
        ));
        egui::ScrollArea::both().show(ui, |ui| {
            let script_editor = TextEdit::multiline(&mut self.code_str)
                .code_editor()
                .lock_focus(true)
                .desired_width(f32::INFINITY)
                .desired_rows(30)
                .show(ui);
            if let Some(range) = script_editor.cursor_range {
                self.cursor_range = Some(range);
            }
        });

        if let Some(rx) = self.code_receiver.as_ref() {
            if let Ok(res) = rx.try_recv() {
                self.mode = RecordMode::Interact;
                info!(msg = "run script done", res = ?res);
                self.code_receiver = None;
                if let Err(e) = res {
                    self.logs_toasts
                        .push((Level::ERROR, format!("script run failed: {:?}", e)));
                }
            }
        }
        ui.add_enabled_ui(self.code_receiver.is_none(), |ui| {
            ui.horizontal(|ui| {
                if ui.button("run script").clicked() {
                    let code = self.code_str.clone();
                    let (tx, rx) = channel();
                    self.code_receiver = Some(rx);

                    let msg_tx = self.api.tx.clone();
                    info!(msg = "run script");
                    self.mode = RecordMode::View;
                    thread::spawn(move || {
                        let res = t_binding::JSEngine::new(msg_tx).run_string(code.as_str());
                        tx.send(res)
                    });
                }
                if self.code_receiver.is_some() {
                    ui.spinner();
                }
            });
        });
    }

    fn render_rect(ui: &mut egui::Ui, rects: &mut Vec<DragedRect>) {
        let mut delete_rects = Vec::new();
        for (i, DragedRect { hover, rect, click }) in rects.iter_mut().rev().enumerate() {
            *hover = ui
                .group(|ui| {
                    ui.horizontal(|ui| {
                        if ui.button("delete").clicked() {
                            delete_rects.push(i);
                        };
                        ui.label(format!(
                            "rect : l:{:.1?} t:{:.1?} w:{:.1?} h:{:.1?}",
                            rect.left, rect.top, rect.width, rect.height
                        ));
                    });
                    if let Some((x, y)) = click {
                        let mut delated = false;
                        ui.horizontal(|ui| {
                            if ui.button("delete").clicked() {
                                delated = true;
                            };
                            ui.label(format!("point: x:{:.1?}, y:{:.1?}", x, y));
                        });
                        if delated {
                            *click = None;
                        }
                    }
                })
                .response
                .hovered();
        }
        // handle delete action
        let mut index: usize = rects.len();
        rects.retain(|_| {
            index -= 1;
            !delete_rects.contains(&index)
        });
    }

    fn render_needles(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(
                self.config
                    .as_ref()
                    .map(|c| c.vnc.is_some())
                    .unwrap_or_default(),
                |ui| ui.selectable_value(&mut self.mode, RecordMode::Interact, "Vnc"),
            );
            ui.add_enabled_ui(
                self.config
                    .as_ref()
                    .map(|c| c.vnc.is_some())
                    .unwrap_or_default(),
                |ui| {
                    if ui
                        .selectable_value(&mut self.mode, RecordMode::Edit, "Needle Edit")
                        .clicked()
                    {
                        if let Err(e) = self.api.vnc_mouse_hide() {
                            self.logs_toasts.push((
                                Level::ERROR,
                                format!("mouse hide failed, reason = {:?}", e),
                            ));
                        }
                        self.current_screenshot = self.screenshots.read().back().map(|x| x.clone());
                    }
                },
            );
            ui.add_enabled_ui(false, |ui| {
                ui.selectable_value(&mut self.mode, RecordMode::View, "View")
            });
        });

        match self.mode {
            RecordMode::Interact => {}
            RecordMode::Edit => {
                ui.separator();
                let needle_dir = self
                    .config
                    .as_ref()
                    .and_then(|c| c.vnc.as_ref().and_then(|c| c.needle_dir.as_ref()))
                    .and_then(|s| PathBuf::from_str(s).ok());

                let needle_dir_clone = needle_dir.clone();
                ui.vertical(|ui| {
                    // needle dir path
                    if let Some(dir) = needle_dir_clone {
                        ui.colored_label(
                            Color32::GREEN,
                            format!("folder: {}", dir.to_string_lossy()),
                        );
                    } else {
                        ui.colored_label(
                            Color32::RED,
                            "folder: Please set needle dir in your config file",
                        );
                    }
                });

                ui.group(|ui| {
                    // needle name
                    ui.text_edit_singleline(&mut self.needle_name);
                    // save button
                    if ui.button("save needle").clicked() {
                        match needle_dir.as_ref() {
                            Some(needle_dir) => match self.current_screenshot.take() {
                                Some(s) => {
                                    if !self.needle_name.is_empty() {
                                        if let Some(rects) = self.drag_rects.take() {
                                            let needle = NeedleSource {
                                                screenshot: s.clone(),
                                                rects,
                                                name: self.needle_name.clone(),
                                            };
                                            if needle.save_to_file(needle_dir).is_ok() {
                                                self.needles.push(needle);
                                                self.mode = RecordMode::Interact;
                                                self.logs_toasts.push((
                                                    Level::INFO,
                                                    "save needle success".to_string(),
                                                ));
                                            } else {
                                                self.drag_rects = Some(needle.rects);
                                                self.logs_toasts.push((
                                                    Level::ERROR,
                                                    "save needle failed".to_string(),
                                                ));
                                            }
                                        } else {
                                            self.logs_toasts.push((
                                                Level::ERROR,
                                                "no area selected".to_string(),
                                            ));
                                        }
                                    } else {
                                        self.logs_toasts.push((
                                            Level::ERROR,
                                            "needle name is empty".to_string(),
                                        ));
                                    }
                                }
                                None => todo!(),
                            },
                            None => {
                                self.logs_toasts.push((
                                    Level::ERROR,
                                    "folder: Please set needle dir in your config file".to_string(),
                                ));
                            }
                        }
                    }

                    if let Some(rects) = self.drag_rects.as_mut() {
                        ui.vertical(|ui| Self::render_rect(ui, rects));
                    }
                });
            }
            RecordMode::View => {}
        }

        ui.colored_label(
            Color32::LIGHT_BLUE,
            RichText::heading(RichText::new("needles")),
        );
        for NeedleSource {
            screenshot: _,
            rects,
            name,
        } in self.needles.iter_mut()
        {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("tag: {}", name)).text_style(egui::TextStyle::Heading),
                );
                Self::render_rect(ui, rects)
            });
        }
    }

    fn render_file(&mut self, ui: &mut egui::Ui, path: &PathBuf) {
        self.file_watcher.try_watch(path);
        if let Some(file_content) = self.file_watcher.cache.read().get(path) {
            // let pathname = path.as_path().display();
            // warn!(msg = "watcher received event", path = ?pathname);
            // let mut file_content = fs::read_to_string(&path).unwrap_or_default();
            egui::ScrollArea::both().show(ui, |ui| {
                ui.columns(1, |cols| {
                    let left = &mut cols[0];
                    let start = Instant::now();

                    // TableBuilder::new(left)
                    //     .striped(true)
                    //     .resizable(true)
                    //     .column(Column::auto().resizable(true))
                    //     .column(Column::remainder())
                    //     .header(20., |mut header| {
                    //         header.col(|ui| {
                    //             ui.heading("line");
                    //         });
                    //         header.col(|ui| {
                    //             ui.heading("content");
                    //         });
                    //     })
                    //     .body(|mut body| {
                    //         for (i, line) in file_content.iter().enumerate() {
                    //             body.row(20.0, |mut row| {
                    //                 row.col(|ui| {
                    //                     ui.label(format!("{}", i + 1));
                    //                 });
                    //                 row.col(|ui| {
                    //                     ui.label(line.as_str());
                    //                 });
                    //             });
                    //         }
                    //     });
                    TextEdit::multiline(&mut file_content.as_str())
                        .desired_width(f32::INFINITY)
                        .code_editor()
                        .hint_text("empty file, waiting content...")
                        .interactive(false)
                        .show(left);
                    debug!("multiline: {:?}", start.elapsed().as_millis());
                    // let right = &mut cols[1];
                    // TextEdit::multiline(&mut stripped)
                    //     .desired_width(f32::INFINITY)
                    //     .code_editor()
                    //     .interactive(false)
                    //     .show(right);
                })
            });
        }
    }
}

impl eframe::App for Recorder {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // receive new screenshot
        self.pre_frame();

        // render ui
        egui::TopBottomPanel::top("tool bar").show(ctx, |ui| {
            self.render_top_bar(ui);
        });

        egui::TopBottomPanel::bottom("status bar").show(ctx, |ui| {
            ctx.texture_ui(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::TopBottomPanel::top("top_panel")
                .resizable(true)
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Top Panel");
                        if ui.button("Config").clicked() {
                            self.show_config_edit_window = true;
                        }

                        let size = ctx.screen_rect();
                        egui::Window::new("Config")
                            .open(&mut self.show_config_edit_window)
                            .collapsible(false)
                            .resizable(true)
                            .movable(true)
                            .pivot(egui::Align2::CENTER_CENTER)
                            .default_pos(Pos2 {
                                x: (size.min.x + size.max.x) / 2.,
                                y: (size.min.y + size.max.y) / 2.,
                            })
                            .show(ctx, |ui| {
                                TextEdit::multiline(&mut self.config_str)
                                    .code_editor()
                                    .lock_focus(true)
                                    .desired_width(640.)
                                    .desired_rows(40)
                                    .ui(ui);
                                if ui.button("try connect").clicked() {
                                    if let Err(e) = self.api.set_config(self.config_str.to_string())
                                    {
                                        self.logs_toasts
                                            .push((Level::ERROR, format!("connect failed, {}", e)));
                                    } else {
                                        self.config =
                                            t_config::Config::from_toml_str(&self.config_str).ok();
                                        self.logs_toasts
                                            .push((Level::INFO, "connect success!".to_string()));
                                    }
                                };
                            });
                    })
                });

            egui::SidePanel::left("left_panel")
                .resizable(true)
                .default_width(300.0)
                .width_range(300.0..)
                .show_inside(ui, |ui| {
                    ui.vertical_centered(|ui| self.render_code_editor(ui));
                });

            egui::SidePanel::right("right_panel")
                .resizable(true)
                .default_width(300.)
                .show_inside(ui, |ui| {
                    ui.vertical_centered(|ui| self.render_needles(ui));
                });

            egui::TopBottomPanel::bottom("bottom_panel")
                .resizable(true)
                .default_height(220.)
                .show_inside(ui, |ui| {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.vertical(|ui| {
                            // ui.heading("Bottom Panel");
                            self.render_logs(ui)
                        });
                    })
                });

            egui::CentralPanel::default()
                .frame(egui::containers::Frame {
                    // fill: Color32::LIGHT_BLUE,
                    inner_margin: Margin {
                        left: 0.,
                        right: 0.,
                        top: 0.,
                        bottom: 0.,
                    },
                    ..Default::default()
                })
                .show_inside(ui, |ui| {
                    // ui.heading("Central Panel");
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(
                            self.config
                                .as_ref()
                                .map(|c| c.vnc.is_some())
                                .unwrap_or_default(),
                            |ui| ui.selectable_value(&mut self.tab, Tab::Vnc, "Vnc"),
                        );
                        ui.add_enabled_ui(
                            self.config
                                .as_ref()
                                .map(|c| c.ssh.is_some())
                                .unwrap_or_default(),
                            |ui| ui.selectable_value(&mut self.tab, Tab::Ssh, "Ssh"),
                        );
                        ui.add_enabled_ui(
                            self.config
                                .as_ref()
                                .map(|c| c.serial.is_some())
                                .unwrap_or_default(),
                            |ui| ui.selectable_value(&mut self.tab, Tab::Serial, "Serial"),
                        );
                    });
                    match self.tab {
                        Tab::Vnc => self.render_vnc(ui),
                        Tab::Serial => {
                            let serial_log_file = self
                                .config
                                .as_ref()
                                .and_then(|c| c.serial.as_ref().and_then(|c| c.log_file.clone()));
                            if let Some(path) = serial_log_file {
                                self.render_file(ui, &path)
                            }
                        }
                        Tab::Ssh => {
                            let serial_log_file = self
                                .config
                                .as_ref()
                                .and_then(|c| c.ssh.as_ref().and_then(|c| c.log_file.clone()));
                            if let Some(path) = serial_log_file {
                                self.render_file(ui, &path)
                            }
                        }
                    };
                });
        });

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
                .default_pos(Pos2 {
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
