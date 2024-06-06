// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use crate::gui::RecordMode;

use super::{
    state::{PanelState, Screenshot},
    SharedState, CAPS_MAP,
};
use chrono::Local;
use eframe::egui::{
    self,
    ahash::{HashMap, HashMapExt},
    text::CursorRange,
    Layout, RichText, Sense, TextEdit, Widget,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        mpsc::{channel, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use t_binding::api::{Api, RustApi};
use t_runner::{error::DriverError, DriverBuilder};
use tracing::{debug, info};
use tracing_core::Level;

pub struct FileWatcher {
    cache: Arc<parking_lot::RwLock<HashMap<PathBuf, Vec<String>>>>,
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
                // lock.insert(path.clone(), file);
                lock.insert(path.clone(), file.lines().map(|s| s.to_string()).collect());
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
                                stripped.lines().map(|s| s.to_string()).collect(),
                                // stripped.to_string(),
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

pub struct Viewer {
    // viewer
    pub share_state: Arc<SharedState>,
    file_watcher: FileWatcher,

    // screenshot
    code_receiver: Option<Receiver<Result<(), String>>>,
    cursor_range: Option<CursorRange>,

    last_move_interval: Instant,
    minimal_move_interval: Duration,
}

impl Viewer {
    pub fn new() -> Self {
        Self {
            // only used in PNG to egui::ColorImage, take more cpu usage
            share_state: Arc::new(SharedState::new()),
            code_receiver: None,

            cursor_range: None,

            // file
            file_watcher: FileWatcher::new(),

            last_move_interval: Instant::now(),
            minimal_move_interval: Duration::from_millis(50),
        }
    }

    pub fn connect_backend(
        &self,
        ctx: egui::Context,
        state: &mut PanelState,
    ) -> Result<(), DriverError> {
        let shared_state = self.share_state.clone();
        let builder = DriverBuilder::new(state.config.clone());
        let mut d = builder.build()?;
        d.start();
        state.driver = Some((RustApi::new(d.msg_tx), d.stop_tx));

        let Some((api, _)) = state.driver.as_ref() else {
            return Ok(());
        };
        let api = api.clone();

        thread::spawn(move || {
            let interval = shared_state.frame_status.read().screenshot_interval;
            loop {
                // if already got new screenshot in this egui frame, then skip
                if let Some(screenshot_interval) = interval {
                    if Instant::now()
                        < shared_state.frame_status.read().last_screenshot + screenshot_interval
                    {
                        continue;
                    }
                }

                if let Ok(screenshot) = api.vnc_get_screenshot() {
                    // update status
                    shared_state.frame_status.write().last_screenshot = Instant::now();
                    shared_state.sample_status.write().screenshot_count += 1;

                    if shared_state.screen.read().is_none() {
                        // append new screenshot
                        let s = Screenshot::new(
                            screenshot,
                            &ctx,
                            *shared_state.use_rayon.read(),
                            Local::now(),
                        );
                        *shared_state.screen.write() = Some(s);
                    } else if let Some(s) = shared_state.screen.write().as_mut() {
                        s.update(screenshot);
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
        Ok(())
    }

    pub fn ui_render(&mut self, ui: &mut egui::Ui, state: &mut PanelState) {
        {
            let lock = self.share_state.screen.read();
            let Some(screenshot) = lock.as_ref() else {
                return;
            };

            // render current screenshot
            let img = screenshot.image();
            let screenshot = ui.add(img.sense(Sense::click_and_drag()));

            let Some((api, _)) = state.driver.as_ref() else {
                return;
            };

            // if mouse move out of image, do nothing
            if let Some(pos) = screenshot.hover_pos() {
                let relative_x = (pos.x as u16).saturating_sub(screenshot.rect.left() as u16);
                let relative_y = (pos.y as u16).saturating_sub(screenshot.rect.top() as u16);

                if Instant::now() - self.last_move_interval > self.minimal_move_interval {
                    if api.vnc_mouse_move(relative_x, relative_y).is_err() {
                        // FIXME: too many error log
                        // self.logs_toasts.push((
                        //     Level::ERROR,
                        //     format!("mouse move failed, reason = {:?}", e),
                        // ));
                    }
                    self.last_move_interval = Instant::now();
                }

                ui.input(|i| {
                    for e in i.events.iter() {
                        match e {
                            // TODO: It seems easier to copy locally and paste remotely, but what about the other way around?
                            // egui::Event::Copy => todo!(),
                            // egui::Event::Cut => todo!(),
                            // egui::Event::Paste(_) => todo!(),
                            egui::Event::Text(s) => {
                                for c in s.as_bytes() {
                                    if let Some(v) = CAPS_MAP.get(c) {
                                        // if c is with capsLk, send key with shift-key, if not, just key
                                        let mut keys = String::new();
                                        if *c != *v {
                                            keys.push_str("shift-");
                                        }
                                        keys.push(*c as char);
                                        debug!(msg = "text input", text = keys);
                                        let _ = api.vnc_send_key(keys);
                                    }
                                }
                            } // Event::Key would be enough?
                            egui::Event::Key {
                                key,
                                physical_key: _,
                                pressed,
                                repeat: _, // no repeaat
                                modifiers,
                            } => {
                                if *pressed
                                    && !(
                                        // ascii direct or with shift
                                        (*key >= egui::Key::Colon && *key <= egui::Key::Z)
                                            && (modifiers.is_none() || modifiers.shift_only())
                                    )
                                {
                                    let mut keys = "".to_string();
                                    if modifiers.ctrl {
                                        keys.push_str("ctrl-");
                                    }
                                    if modifiers.alt {
                                        keys.push_str("alt-");
                                    }
                                    if modifiers.shift {
                                        keys.push_str("shift-");
                                    }
                                    keys.push_str(key.name());
                                    debug!(msg = "key input", final_key = keys.to_string());
                                    let _ = api.vnc_send_key(keys);
                                }
                            }
                            _ => {}
                        }
                    }
                });
            }

            // handle drag
            if let Some(_pos) = screenshot.interact_pointer_pos() {
                let relative_x = (_pos.x as u16).saturating_sub(screenshot.rect.left() as u16);
                let relative_y = (_pos.y as u16).saturating_sub(screenshot.rect.top() as u16);

                if screenshot.drag_started() {
                    // init current pos
                    let _ = api.vnc_mouse_keydown();
                    let _ = api.vnc_mouse_drag(relative_x, relative_y);
                } else if screenshot.dragged() {
                    let _ = api.vnc_mouse_drag(relative_x, relative_y);
                } else if screenshot.drag_stopped() {
                    let _ = api.vnc_mouse_keyup();
                }

                if screenshot.clicked() {
                    if let Err(e) = api.vnc_mouse_click() {
                        state.logs_toasts.push((
                            Level::ERROR,
                            format!("mouse click failed, reason = {:?}", e),
                        ));
                    }
                }

                if screenshot.secondary_clicked() {
                    if let Err(e) = api.vnc_mouse_rclick() {
                        state.logs_toasts.push((
                            Level::ERROR,
                            format!("mouse right click failed, reason = {:?}", e),
                        ));
                    }
                }
            }
        }
    }

    pub fn render_code_editor(&mut self, ui: &mut egui::Ui, state: &mut PanelState) {
        // code editor
        ui.label(format!(
            "selected: {:?}",
            self.cursor_range.map(|r| r.as_sorted_char_range())
        ));
        egui::ScrollArea::both().show(ui, |ui| {
            let script_editor = TextEdit::multiline(&mut state.code_str)
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
                state.mode = RecordMode::Interact;
                info!(msg = "run script done", res = ?res);
                self.code_receiver = None;
                if let Err(e) = res {
                    state
                        .logs_toasts
                        .push((Level::ERROR, format!("script run failed: {:?}", e)));
                }
            }
        }
        ui.add_enabled_ui(self.code_receiver.is_none(), |ui| {
            ui.horizontal(|ui| {
                if ui.button("run script").clicked() {
                    let code = state.code_str.clone();
                    let (tx, rx) = channel();
                    self.code_receiver = Some(rx);

                    let Some((api, _)) = state.driver.as_ref() else {
                        return;
                    };

                    let msg_tx = api.tx.clone();
                    info!(msg = "run script");
                    state.mode = RecordMode::View;
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

    pub fn render_file(&mut self, ui: &mut egui::Ui, path: &PathBuf) {
        self.file_watcher.try_watch(path);
        if let Some(file_content) = self.file_watcher.cache.read().get(path) {
            // let pathname = path.as_path().display();
            // warn!(msg = "watcher received event", path = ?pathname);
            // let mut file_content = fs::read_to_string(&path).unwrap_or_default();
            egui::ScrollArea::both().show_viewport(ui, |ui, _rect| {
                let start = Instant::now();
                let line_height = ui.text_style_height(&egui::TextStyle::Monospace);
                egui_extras::TableBuilder::new(ui)
                    .resizable(true)
                    .column(egui_extras::Column::auto_with_initial_suggestion(30.).resizable(true))
                    .column(egui_extras::Column::remainder())
                    .body(|body| {
                        body.rows(line_height, file_content.len(), |mut row| {
                            let i = row.index();
                            row.col(|ui| {
                                ui.with_layout(
                                    Layout::right_to_left(egui::Align::Center),
                                    |ui: &mut egui::Ui| {
                                        let index_str = format!("{}", i + 1);
                                        egui::Label::new(RichText::new(index_str).code())
                                            .wrap(false)
                                            .selectable(false)
                                            .ui(ui);
                                    },
                                );
                            });
                            row.col(|ui| {
                                egui::Label::new(RichText::new(&file_content[i]).code())
                                    .wrap(false)
                                    .selectable(true)
                                    .ui(ui);
                            });
                        });
                    });
                debug!("multiline: {:?}", start.elapsed().as_millis());
            });
        }
    }
}
