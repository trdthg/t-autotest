use eframe::egui::Color32;
use egui_notify::ToastLevel;

pub fn tracing_level_2_egui_color32(level: &tracing_core::Level) -> Option<Color32> {
    match *level {
        tracing_core::Level::ERROR => Some(Color32::RED),
        tracing_core::Level::WARN => Some(Color32::YELLOW),
        _ => None,
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
