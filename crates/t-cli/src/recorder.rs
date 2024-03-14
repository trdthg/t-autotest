// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::{
    collections::LinkedList,
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver, Sender},
    time::{self, Duration, Instant, UNIX_EPOCH},
};

use eframe::egui::{
    self, Color32, ColorImage, Margin, Pos2, RichText, Sense, Stroke, TextureHandle, TextureOptions,
};
use image::DynamicImage;
use t_binding::api;
use t_console::PNG;
use t_runner::needle::{Needle, NeedleConfig};
use tracing::{debug, error, info, warn};

enum RecordMode {
    Edit,
    Interact,
}

struct Screenshot {
    source: PNG,
    image: Option<TextureHandle>,
    thumbnail: Option<TextureHandle>,
}

impl Screenshot {
    fn clone_source(&self) -> Self {
        Self {
            source: self.source.clone(),
            image: None,
            thumbnail: None,
        }
    }

    fn image(&mut self, ctx: &egui::Context) -> egui::Image {
        let sized_image = match &self.image {
            None => {
                // update screenshot
                let color_image = to_egui_rgb_color_image(&self.source);
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

    #[allow(unused)]
    fn thumbnail(&mut self, ctx: &egui::Context) -> egui::Image {
        if let Some(thuma) = self.thumbnail.as_ref() {
            let sized_image = egui::load::SizedTexture::new(thuma.id(), thuma.size_vec2());
            egui::Image::from_texture(sized_image)
        } else {
            // TODO: generate thumbnail if not exists
            self.image(ctx)
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
            ()
        })?;
        Ok(())
    }
}

struct FrameStatus {
    // phy frame
    phy_frame_start: Instant,
    // egui render frame
    render_frame_start: Instant,
    last_screenshot: Instant,
    new_screenshot_count: usize,
}

impl Default for FrameStatus {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            phy_frame_start: now,
            render_frame_start: now,
            last_screenshot: now,
            new_screenshot_count: Default::default(),
        }
    }
}

pub struct Recorder {
    show_confirmation_dialog: bool,
    allowed_to_close: bool,
    stop_tx: Sender<()>,

    // frame evenv count
    frame_status: FrameStatus,

    // screenshot
    mode: RecordMode,

    // screenshots
    max_screenshot_num: usize,
    screenshot_rx: Receiver<PNG>,
    screenshots: std::collections::LinkedList<Screenshot>,

    // edit mode
    type_string: String,
    send_key: String,

    // interact mode
    needle_dir: String,
    needle_name: String,
    drag_rect: Option<RectF32>,
    drag_rects: Option<Vec<DragedRect>>,
    editting_ss: Option<Screenshot>,
    needles: Vec<NeedleSource>,
}

struct NeedleSource {
    screenshot: Screenshot,
    rects: Vec<DragedRect>,
    name: String,
}

impl NeedleSource {
    pub fn save_to_file(&self, dir: Option<String>) -> Result<(), ()> {
        let dir = dir.unwrap_or(".".to_string());
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
    screenshot_rx: Receiver<PNG>,

    // option
    max_screenshot_num: usize,
    needle_dir: Option<String>,
}

impl RecorderBuilder {
    pub fn new(stop_tx: Sender<()>, screenshot_rx: Receiver<PNG>) -> Self {
        Self {
            stop_tx,
            screenshot_rx,
            max_screenshot_num: 20,
            needle_dir: None,
        }
    }

    pub fn with_max_screenshots(mut self, num: usize) -> Self {
        self.max_screenshot_num = num;
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

            frame_status: Default::default(),

            stop_tx: self.stop_tx,
            screenshot_rx: self.screenshot_rx,
            mode: RecordMode::Interact,

            max_screenshot_num: self.max_screenshot_num,
            screenshots: LinkedList::new(),

            // control
            type_string: String::new(),
            send_key: String::new(),

            // edit
            drag_rects: None,
            drag_rect: None,
            editting_ss: None,
            needle_dir: String::new(),
            needles: Vec::new(),
            needle_name: String::new(),
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
    pub fn transform_noreverse(&mut self, x: f32, y: f32) {
        self.width += x;
        self.height += y;
    }

    pub fn check(&mut self) {
        self.transform(0., 0.);
    }

    fn transform(&mut self, x: f32, y: f32) {
        let Self {
            left,
            top,
            width,
            height,
        } = self;
        Self::transform_one(left, width, x);
        Self::transform_one(top, height, y);
    }

    fn transform_one(left: &mut f32, width: &mut f32, x: f32) {
        let l = *left as f32;
        let mut r = l + *width as f32;
        r += x;

        let mut new_l = l.min(r);
        let new_r = l.max(r);
        if new_l < 0. {
            new_l = 0.;
        }
        *left = new_l;
        *width = new_r - new_l;
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
    r.transform(-1., -1.);
    assert!(r.left == 1.);
    assert!(r.top == 1.);
    assert!(r.width == 1.);
    assert!(r.height == 1.);

    r.transform(5., 5.);

    assert!(r.left == 2.);
    assert!(r.top == 2.);
    assert!(r.width == 4.);
    assert!(r.height == 4.);

    r.transform(-5., -5.);
    assert!(r.left == 1.);
    assert!(r.top == 1.);
    assert!(r.width == 1.);
    assert!(r.height == 1.);

    r.transform(-2., -2.);
    assert!(r.left == 0.);
    assert!(r.top == 0.);
    assert!(r.width == 1.);
    assert!(r.height == 1.);
}

#[derive(Debug, Clone, Copy)]
struct DragedRect {
    pub hover: bool,
    pub rect: RectF32,
}

fn to_egui_rgb_color_image(image: &PNG) -> ColorImage {
    let color_image =
        egui::ColorImage::from_rgb([image.width as usize, image.height as usize], &image.data);
    color_image
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
        let FRAME_MS = Duration::from_secs_f32(1. / 30.);
        self.frame_status.render_frame_start = Instant::now();

        // handle vnc subscribe
        if let Ok(screenshot) = self.screenshot_rx.try_recv() {
            // 60 fps
            // if already got new screenshot in this frame, then skip
            if self.frame_status.last_screenshot > self.frame_status.phy_frame_start {
                return;
            }
            // append new screenshot
            // update status
            self.frame_status.last_screenshot = Instant::now();
            self.frame_status.new_screenshot_count += 1;

            // handle too many
            if self.screenshots.len() == self.max_screenshot_num {
                self.screenshots.pop_front();
            }

            let s = Screenshot {
                source: screenshot,
                image: None,
                thumbnail: None,
            };
            self.screenshots.push_back(s);
        }
    }

    fn after_frame(&mut self, ctx: &egui::Context) {
        let FRAME_MS = Duration::from_secs_f32(1. / 20.);

        let render_frame_elasped = Instant::now() - self.frame_status.render_frame_start;
        if render_frame_elasped > FRAME_MS {
            warn!("frame render take {} ms", render_frame_elasped.as_millis());
        }

        while Instant::now() - self.frame_status.phy_frame_start > FRAME_MS {
            self.frame_status.phy_frame_start += FRAME_MS;
            self.frame_status.new_screenshot_count = 0;

            debug!(
                "this frame receive {} new screenshot",
                self.frame_status.new_screenshot_count
            );
        }

        ctx.request_repaint_after(FRAME_MS);
    }
}

impl eframe::App for Recorder {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // receive new screenshot
        self.pre_frame(ctx);

        egui::CentralPanel::default()
            .frame(egui::containers::Frame {
                // fill: Color32::LIGHT_BLUE,
                inner_margin: Margin {
                    left: 20.,
                    right: 20.,
                    top: 20.,
                    bottom: 20.,
                },
                stroke: Stroke::new(
                    2.,
                    match self.mode {
                        RecordMode::Edit => Color32::YELLOW,
                        RecordMode::Interact => Color32::BLUE,
                    },
                ),
                ..Default::default()
            })
            .show(ctx, |ui| {
                ui.set_width(1280.);
                ui.set_height(800.);
                egui::ScrollArea::both().show(ui, |ui| {
                    match self.mode {
                        RecordMode::Interact => {
                            if let Some(screenshot) = self.screenshots.back_mut() {
                                // render current screenshot
                                let screenshot =
                                    ui.add(screenshot.image(ctx).sense(Sense::click()));
                                if screenshot.clicked() {
                                    // TODO:
                                    if let Some(pos) = screenshot.interact_pointer_pos() {
                                        if let Err(e) = api::vnc_mouse_move(
                                            pos.x as u16 - screenshot.rect.left() as u16,
                                            pos.y as u16 - screenshot.rect.top() as u16,
                                        ) {
                                            error!("mouse move failed");
                                        }
                                        if let Err(e) = api::vnc_mouse_click() {
                                            error!("click click failed");
                                        }
                                    }
                                }
                            }
                        }
                        RecordMode::Edit => {
                            // handle select event
                            if self.editting_ss.is_none() {
                                if let Some(screenshot) = self.screenshots.back() {
                                    self.editting_ss = Some(Screenshot {
                                        source: screenshot.source.clone(),
                                        image: None,
                                        thumbnail: None,
                                    });
                                }
                            }

                            if let Some(screenshot) = &mut self.editting_ss {
                                let screenshot = ui.add(screenshot.image(ctx).sense(Sense::drag()));
                                if screenshot.drag_started() {
                                    if self.drag_rect.is_none() {
                                        println!(
                                            "drag_rect inited {:?}",
                                            screenshot.interact_pointer_pos()
                                        );
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
                                } else if screenshot.drag_released() {
                                    // self.drag_rects.push(drag_rect);
                                    if let Some(mut rect) = self.drag_rect.take() {
                                        rect.check();
                                        if rect.width != 0. && rect.height != 0. {
                                            println!("final: {:?}", rect);
                                            if self.drag_rects.is_none() {
                                                self.drag_rects = Some(Vec::new());
                                            }
                                            if let Some(rects) = self.drag_rects.as_mut() {
                                                rects.push(DragedRect { hover: false, rect });
                                            }
                                        }
                                    }
                                } else if let Some(rect) = self.drag_rect.as_mut() {
                                    let delta = screenshot.drag_delta();
                                    rect.transform_noreverse(delta.x, delta.y);
                                }

                                // draw selected rect
                                if let Some(rects) = self.drag_rects.as_ref() {
                                    for DragedRect { hover, rect } in rects.iter() {
                                        let rect = egui::Rect {
                                            min: Pos2 {
                                                x: rect.left as f32 + screenshot.rect.left(),
                                                y: rect.top as f32 + screenshot.rect.top(),
                                            },
                                            max: Pos2 {
                                                x: (rect.left + rect.width) as f32
                                                    + screenshot.rect.left(),
                                                y: (rect.top + rect.height) as f32
                                                    + screenshot.rect.top(),
                                            },
                                        };
                                        // mesh.add_colored_rect(rect, Color32::LIGHT_BLUE);
                                        ui.painter().rect_stroke(
                                            rect,
                                            0.0,
                                            egui::Stroke::new(
                                                2.0,
                                                if *hover { Color32::RED } else { Color32::GREEN },
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                })
            });

        // control bar and needle list
        egui::SidePanel::right("control panel").show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_source("control panel")
                .show(ui, |ui| {
                    ui.set_height(ui.available_height() / 2.);

                    if ui
                        .button(match self.mode {
                            RecordMode::Edit => "vnc client",
                            RecordMode::Interact => "edit mode",
                        })
                        .clicked()
                    {
                        match self.mode {
                            RecordMode::Edit => self.mode = RecordMode::Interact,
                            RecordMode::Interact => self.mode = RecordMode::Edit,
                        }
                    };

                    match self.mode {
                        RecordMode::Edit => {}
                        RecordMode::Interact => {
                            ui.text_edit_singleline(&mut self.type_string);
                            if ui.button("send").clicked() {
                                api::vnc_type_string(self.type_string.clone());
                            }

                            ui.text_edit_singleline(&mut self.send_key);
                            if ui.button("send").clicked() {
                                api::vnc_send_key(self.send_key.clone());
                            }
                        }
                    }
                });
            // needle list
            egui::ScrollArea::vertical()
                .id_source("needle view")
                .show(ui, |ui| {
                    ui.set_height(ui.available_height() / 2.);

                    ui.text_edit_singleline(&mut self.needle_dir);

                    ui.text_edit_singleline(&mut self.needle_name);

                    if let Some(s) = self.editting_ss.take() {
                        if ui.button("save needle").clicked() {
                            if !self.needle_name.is_empty() {
                                if let Some(rects) = self.drag_rects.take() {
                                    let needle = NeedleSource {
                                        screenshot: s.clone_source(),
                                        rects: rects,
                                        name: self.needle_name.clone(),
                                    };
                                    if  needle.save_to_file(Some(self.needle_dir.clone())).is_ok() {
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
                                        // label
                                        if ui
                                            .label(format!(
                                                "l:{:.1?} t:{:.1?} w:{:.1?} h:{:.1?}",
                                                rect.left, rect.top, rect.width, rect.height
                                            ))
                                            .hovered()
                                        {
                                            *hover = true;
                                        } else {
                                            *hover = false;
                                        }
                                        // delete
                                        if ui.button("delete").clicked() {
                                            delete_rects.push(i);
                                        };
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

                    ui.add_space(20.);
                    ui.heading("saved needles");
                    ui.add_space(20.);

                    for NeedleSource {
                        screenshot,
                        rects,
                        name,
                    } in self.needles.iter_mut()
                    {

                        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {

                            ui.label(RichText::new(format!("tag: {}", name)).text_style(egui::TextStyle::Heading));

                            let mut delete_rects = Vec::new();
                            for (i, DragedRect { hover, rect }) in
                                rects.iter_mut().rev().enumerate()
                            {
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::LEFT),
                                    |ui| {
                                        // label
                                        if ui
                                            .label(format!(
                                                "    rect: {{l:{:.1?}, t:{:.1?}, w:{:.1?}, h:{:.1?}}}",
                                                rect.left, rect.top, rect.width, rect.height
                                            ))
                                            .hovered()
                                        {
                                            *hover = true;
                                        } else {
                                            *hover = false;
                                        }
                                        // delete
                                        if ui.button("delete").clicked() {
                                            delete_rects.push(i);
                                        };
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
                });
        });

        // screenshots
        egui::TopBottomPanel::bottom("screenshots").show(ctx, |ui| {
            // ui.set_max_height(ui.available_height() / 4.);
            ui.set_max_height(200.);
            ui.heading(format!(
                "screenshot buffer count: {}",
                self.screenshots.len()
            ));
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    for (_i, screenshot) in self.screenshots.iter_mut().rev().enumerate() {
                        ui.add(screenshot.image(ctx).max_height(200.));
                    }
                });
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

        if self.show_confirmation_dialog {
            egui::Window::new("Do you want to quit?")
                .collapsible(false)
                .resizable(false)
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
