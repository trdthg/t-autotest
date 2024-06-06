mod editor;
mod viwer;

// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use editor::NeedleEditor;
use eframe::egui::{self, Color32, Margin, Pos2, RichText, TextEdit, Widget};
use egui_notify::Toast;
use parking_lot::RwLock;
use state::{EguiFrameStatus, PanelState, SampleStatus, Screenshot};
use std::{
    sync::mpsc::Receiver,
    thread,
    time::{Duration, Instant},
};
use t_binding::api::Api;
use t_console::PNG;
use tracing::{debug, error};
use tracing_core::Level;
use util::*;
use viwer::Viewer;
mod state;
mod util;

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

struct SharedState {
    frame_status: RwLock<EguiFrameStatus>,
    sample_status: RwLock<SampleStatus>,
    use_rayon: RwLock<bool>,
    screen: RwLock<Option<Screenshot>>,
}

impl SharedState {
    fn new() -> Self {
        Self {
            frame_status: RwLock::new(EguiFrameStatus::default()),
            sample_status: RwLock::new(SampleStatus::default()),
            use_rayon: RwLock::new(true),
            screen: RwLock::new(None),
        }
    }
}

#[derive(Debug, PartialEq)]
enum LeftPanel {
    ScriptEditor,
    NeedleManager,
    Screenshots,
}

pub struct Gui {
    show_confirmation_dialog: bool,
    allowed_to_close: bool,
    dark_theme: bool,

    state: PanelState,
    show_config_edit_window: bool,

    // panels
    show_panel: bool,
    panel: LeftPanel,

    viwer: Viewer,
    editor: NeedleEditor,

    // logs
    toasts: egui_notify::Toasts,
}

pub struct GuiBuilder {
    screenshot_rx: Option<Receiver<PNG>>,

    // option
    max_screenshot_num: usize,
    config_str: Option<String>,
}

impl GuiBuilder {
    pub fn new(config_str: Option<String>) -> Self {
        Self {
            screenshot_rx: None,
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

    pub fn build(self) -> Gui {
        Gui {
            show_confirmation_dialog: false,
            allowed_to_close: false,
            dark_theme: false,

            show_panel: true,
            panel: LeftPanel::ScriptEditor,

            state: PanelState::new(self.config_str),
            show_config_edit_window: true,

            viwer: Viewer::new(),
            editor: NeedleEditor::new(),

            // logs
            toasts: egui_notify::Toasts::new()
                .with_anchor(egui_notify::Anchor::BottomRight) // 10 units from the bottom right corner
                .with_margin((-10.0, -10.0).into()),
        }
    }
}

impl Gui {
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

impl Gui {
    fn pre_frame(&mut self) {
        self.viwer.share_state.frame_status.write().egui_start = Instant::now();
    }

    fn after_frame(&mut self, ctx: &egui::Context) {
        // handle notify
        while let Some((level, log)) = self.state.logs_toasts.pop_front() {
            let mut toast = Toast::custom(&log, util::tracing_level_2_toast_level(level));
            toast
                .set_duration(Some(Duration::from_secs(3)))
                .set_show_progress_bar(true);
            self.toasts.add(toast);
            self.state.logs_history.push_back((level, log));
        }
        self.toasts.show(ctx);

        let mut sample_status = self.viwer.share_state.sample_status.write();
        let frame_status = self.viwer.share_state.frame_status.read();

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
        let Some((api, _)) = self.state.driver.as_ref() else {
            return;
        };

        ui.horizontal(|ui| {
            if ui.button("force refresh").clicked() && api.vnc_refresh().is_err() {
                self.state
                    .logs_toasts
                    .push((Level::ERROR, "force refresh failed".to_string()));
            }
            let sample_status = self.viwer.share_state.sample_status.read();
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

            let use_rayon = *self.viwer.share_state.use_rayon.read();
            if ui
                .button(format!("rayon: {}", if use_rayon { "on" } else { "off" }))
                .clicked()
            {
                *self.viwer.share_state.use_rayon.write() = !use_rayon;
            }

            ui.colored_label(
                Color32::GREEN,
                RichText::new(format!(
                    "vnc no update:{}s",
                    (Instant::now() - self.viwer.share_state.frame_status.read().last_screenshot)
                        .as_secs()
                ))
                .heading(),
            );
        });
    }

    fn render_vnc(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both()
            .auto_shrink(false)
            .show_viewport(ui, |ui, _rect| match self.state.mode {
                RecordMode::Interact => self.viwer.ui_render(ui, &mut self.state),
                RecordMode::Edit => self.editor.ui_editor(ui, &mut self.state),
                RecordMode::View => {
                    let lock = self.viwer.share_state.screen.read();
                    let Some(screenshot) = lock.as_ref() else {
                        return;
                    };
                    let img = screenshot.image();
                    ui.add(img);
                }
            });
    }

    fn render_logs(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            for (level, log) in self.state.logs_history.iter().rev() {
                let color = tracing_level_2_egui_color32(level);
                ui.colored_label(color, log);
            }
        });
    }

    fn render_screenshorts(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!(
            "screenshot buffer count: {}",
            self.state.screenshots.read().len()
        ));
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut deleted = Vec::new();
            for (i, screenshot) in self.state.screenshots.read().iter().rev().enumerate() {
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
                        self.state.mode = RecordMode::View;
                        self.state.current_screenshot = Some(screenshot.clone());
                    }
                });
                ui.separator();
            }
            let mut index: usize = self.state.screenshots.read().len();
            self.state.screenshots.write().retain(|_| {
                index -= 1;
                !deleted.contains(&index)
            });
        });
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // receive new screenshot
        self.pre_frame();

        // egui::TopBottomPanel::top("status bar").show(ctx, |ui| {
        //     ctx.texture_ui(ui);
        // });

        // render ui
        egui::TopBottomPanel::bottom("tool bar").show(ctx, |ui| {
            self.render_top_bar(ui);
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
            .show(ctx, |ui| {
                egui::TopBottomPanel::top("top_panel")
                    .resizable(true)
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                Color32::LIGHT_BLUE,
                                RichText::new("Nyumbu").heading(),
                            );

                            if ui.button("Config").clicked() {
                                self.show_config_edit_window = true;
                            }

                            if ui.button("theme").clicked() {
                                self.dark_theme = !self.dark_theme;
                                ctx.set_visuals(if self.dark_theme {
                                    egui::Visuals::dark()
                                } else {
                                    egui::Visuals::light()
                                });
                            }

                            if ui.button("left").clicked() {
                                self.show_panel = !self.show_panel;
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
                                    TextEdit::multiline(&mut self.state.config_str)
                                        .code_editor()
                                        .lock_focus(true)
                                        .desired_width(640.)
                                        .desired_rows(40)
                                        .ui(ui);
                                    if ui.button("try connect").clicked() {
                                        self.state.config =
                                            t_config::Config::from_toml_str(&self.state.config_str)
                                                .ok();
                                        if let Err(e) =
                                            self.viwer.connect_backend(ctx.clone(), &mut self.state)
                                        {
                                            self.state
                                                .logs_toasts
                                                .push((Level::ERROR, e.to_string()));
                                        } else {
                                            self.state.logs_toasts.push((
                                                Level::INFO,
                                                "connect success!".to_string(),
                                            ));
                                        }
                                    };
                                });
                        })
                    });

                if self.show_panel {
                    egui::SidePanel::left("left_panel")
                        .resizable(true)
                        .default_width(300.0)
                        .width_range(300.0..)
                        .show_inside(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_enabled_ui(
                                    self.state
                                        .config
                                        .as_ref()
                                        .map(|c| c.vnc.is_some())
                                        .unwrap_or_default(),
                                    |ui| {
                                        ui.selectable_value(
                                            &mut self.state.mode,
                                            RecordMode::Interact,
                                            "Vnc",
                                        )
                                    },
                                );
                                ui.add_enabled_ui(
                                    self.state
                                        .config
                                        .as_ref()
                                        .map(|c| c.vnc.is_some())
                                        .unwrap_or_default(),
                                    |ui| {
                                        if ui
                                            .selectable_value(
                                                &mut self.state.mode,
                                                RecordMode::Edit,
                                                "Needle Edit",
                                            )
                                            .clicked()
                                        {
                                            let Some((api, _)) = self.state.driver.as_ref() else {
                                                return;
                                            };
                                            if let Err(e) = api.vnc_mouse_hide() {
                                                self.state.logs_toasts.push((
                                                    Level::ERROR,
                                                    format!("mouse hide failed, reason = {:?}", e),
                                                ));
                                            }
                                            self.state.current_screenshot = self
                                                .viwer
                                                .share_state
                                                .screen
                                                .read()
                                                .as_ref()
                                                .map(|x| {
                                                    x.clone_new_handle(
                                                        ui.ctx(),
                                                        *self.viwer.share_state.use_rayon.read(),
                                                    )
                                                });
                                        }
                                    },
                                );
                                ui.add_enabled_ui(false, |ui| {
                                    ui.selectable_value(
                                        &mut self.state.mode,
                                        RecordMode::View,
                                        "View",
                                    )
                                });
                            });

                            ui.horizontal(|ui| {
                                ui.selectable_value(
                                    &mut self.panel,
                                    LeftPanel::ScriptEditor,
                                    "Script",
                                );
                                ui.selectable_value(
                                    &mut self.panel,
                                    LeftPanel::NeedleManager,
                                    "Needle",
                                );
                                ui.selectable_value(
                                    &mut self.panel,
                                    LeftPanel::Screenshots,
                                    "Screenshots",
                                );
                            });
                            match self.panel {
                                LeftPanel::ScriptEditor => {
                                    ui.vertical_centered(|ui| {
                                        self.viwer.render_code_editor(ui, &mut self.state)
                                    });
                                }
                                LeftPanel::NeedleManager => {
                                    ui.vertical_centered(|ui| {
                                        self.editor.render_needles(ui, &mut self.state)
                                    });
                                }
                                LeftPanel::Screenshots => self.render_screenshorts(ui),
                            }
                        });
                }

                // egui::SidePanel::right("right_panel")
                //     .resizable(true)
                //     .default_width(300.)
                //     .show_inside(ui, |ui| {});

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

                egui::CentralPanel::default().show_inside(ui, |ui| {
                    // ui.heading("Central Panel");
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(
                            self.state
                                .config
                                .as_ref()
                                .map(|c| c.vnc.is_some())
                                .unwrap_or_default(),
                            |ui| ui.selectable_value(&mut self.state.tab, Tab::Vnc, "Vnc"),
                        );
                        ui.add_enabled_ui(
                            self.state
                                .config
                                .as_ref()
                                .map(|c| c.ssh.is_some())
                                .unwrap_or_default(),
                            |ui| ui.selectable_value(&mut self.state.tab, Tab::Ssh, "Ssh"),
                        );
                        ui.add_enabled_ui(
                            self.state
                                .config
                                .as_ref()
                                .map(|c| c.serial.is_some())
                                .unwrap_or_default(),
                            |ui| ui.selectable_value(&mut self.state.tab, Tab::Serial, "Serial"),
                        );
                    });
                    match self.state.tab {
                        Tab::Vnc => self.render_vnc(ui),
                        Tab::Serial => {
                            let serial_log_file =
                                self.state.config.as_ref().and_then(|c| {
                                    c.serial.as_ref().and_then(|c| c.log_file.clone())
                                });
                            if let Some(path) = serial_log_file {
                                self.viwer.render_file(ui, &path)
                            }
                        }
                        Tab::Ssh => {
                            let serial_log_file = self
                                .state
                                .config
                                .as_ref()
                                .and_then(|c| c.ssh.as_ref().and_then(|c| c.log_file.clone()));
                            if let Some(path) = serial_log_file {
                                self.viwer.render_file(ui, &path)
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
                            self.state.stop();
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
