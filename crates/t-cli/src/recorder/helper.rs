use eframe::egui::{self, Color32, ColorImage, Pos2};
use egui_notify::ToastLevel;
use t_console::PNG;

#[allow(unused)]
pub fn tracing_level_2_egui_color32(level: &tracing_core::Level) -> Color32 {
    match *level {
        tracing_core::Level::ERROR => Color32::RED,
        tracing_core::Level::WARN => Color32::YELLOW,
        tracing_core::Level::INFO => Color32::WHITE,
        tracing_core::Level::DEBUG | tracing_core::Level::TRACE => Color32::GRAY,
        _ => Color32::BLUE,
    }
}

pub fn tracing_level_2_toast_level(level: tracing_core::Level) -> ToastLevel {
    match level {
        tracing_core::Level::ERROR => ToastLevel::Error,
        tracing_core::Level::WARN => ToastLevel::Warning,
        tracing_core::Level::INFO => ToastLevel::Info,
        tracing_core::Level::DEBUG | tracing_core::Level::TRACE => ToastLevel::None,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RectF32 {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
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

    pub fn add_delta_egui_rect(&self, delta: &egui::Rect) -> egui::Rect {
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
pub struct DragedRect {
    pub hover: bool,
    pub rect: RectF32,
    pub click: Option<(f32, f32)>,
}

pub fn to_egui_rgb_color_image(image: &PNG, use_rayon: bool) -> ColorImage {
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
