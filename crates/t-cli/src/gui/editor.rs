use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Vec2};
use t_runner::needle::NeedleConfig;
use tracing::Level;

use super::{
    state::{PanelState, Screenshot},
    DragedRect, RecordMode, RectF32,
};

pub struct NeedleEditor {
    needle_name: String,
    drag_rect: Option<RectF32>,
    drag_rects: Option<Vec<DragedRect>>,
    needles: Vec<NeedleSource>,
}

impl NeedleEditor {
    pub fn new() -> Self {
        Self {
            // edit
            needle_name: String::new(),
            drag_rects: None,
            drag_rect: None,
            needles: Vec::new(),
        }
    }

    pub fn ui_editor(&mut self, ui: &mut egui::Ui, state: &mut PanelState) {
        // handle screenshot
        if let Some(screenshot) = state.current_screenshot.as_mut() {
            // ---------------------------------------------------------------------------------------------------------

            let mut screenshot = ui.add(screenshot.image().sense(Sense::click_and_drag()));

            if let Some(pos_max) = screenshot.hover_pos() {
                let x = pos_max.x - screenshot.rect.left();
                let y = pos_max.y - screenshot.rect.top();
                screenshot =
                    screenshot.on_hover_text_at_pointer(format!("x: {:.1}, y: {:.1}", x, y));
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
                    let rect_res = ui.allocate_rect(draw_rect, Sense::click_and_drag());
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
                                    Color32::from_rgba_premultiplied(255, 255, 255, 120)
                                } else {
                                    Color32::from_rgba_premultiplied(255, 255, 255, 30)
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

    pub fn render_needles(&mut self, ui: &mut egui::Ui, state: &mut PanelState) {
        match state.mode {
            RecordMode::Interact => {}
            RecordMode::Edit => {
                ui.separator();
                let needle_dir = state
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
                            Some(needle_dir) => match state.current_screenshot.as_mut() {
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
                                                state.mode = RecordMode::Interact;
                                                state.logs_toasts.push((
                                                    Level::INFO,
                                                    "save needle success".to_string(),
                                                ));
                                                // save to screenshots list;
                                                // self.share_state.screenshots.write().push_back(s);
                                            } else {
                                                self.drag_rects = Some(needle.rects);
                                                state.logs_toasts.push((
                                                    Level::ERROR,
                                                    "save needle failed".to_string(),
                                                ));
                                            }
                                        } else {
                                            state.logs_toasts.push((
                                                Level::ERROR,
                                                "no area selected".to_string(),
                                            ));
                                        }
                                    } else {
                                        state.logs_toasts.push((
                                            Level::ERROR,
                                            "needle name is empty".to_string(),
                                        ));
                                    }
                                }
                                None => todo!(),
                            },
                            None => {
                                state.logs_toasts.push((
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
