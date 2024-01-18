use std::{fs::File, io::BufReader, path::PathBuf};

use serde::{Deserialize, Serialize};
use t_console::{Rect, PNG};

pub struct NeedleManager {
    dir: String,
}

impl NeedleManager {
    pub fn new(dir: impl Into<String>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn load_by_tag(&self, tag: &str) -> (NeedleConfig, PNG) {
        let needle_path = PathBuf::from_iter(vec![&self.dir, &format!("{tag}.png")]);
        let needle_file = File::open(needle_path).unwrap();
        let needle_png = image::load(BufReader::new(needle_file), image::ImageFormat::Png).unwrap();
        let needle_png = needle_png.into_rgb8();

        let json_path = PathBuf::from_iter(vec![&self.dir, &format!("{tag}.json")]);
        let json_file = File::open(json_path).unwrap();
        let json: NeedleConfig = serde_json::from_reader(BufReader::new(json_file)).unwrap();
        return (json, needle_png);
    }

    #[cfg(test)]
    pub fn load_file_by_tag(&self, tag: &str) -> PNG {
        let needle_path = PathBuf::from_iter(vec![&self.dir, &format!("{tag}.png")]);
        let needle_file = File::open(needle_path).unwrap();
        let needle_png = image::load(BufReader::new(needle_file), image::ImageFormat::Png).unwrap();
        let needle_png = needle_png.into_rgb8();

        return needle_png;
    }

    pub fn cmp_by_tag(&self, s: &PNG, tag: &str) -> bool {
        let (needle_cfg, needle_png) = self.load_by_tag(&tag);
        for area in needle_cfg.area.iter() {
            if !cmp_image_rect(&needle_png, &s, &area.into()) {
                return false;
            }
        }
        return true;
    }
}

pub fn cmp_image_full(img1: &PNG, img2: &PNG) -> bool {
    // 检查图像的宽度和高度是否相同
    if img1.width() != img2.width() || img1.height() != img2.height() {
        return false;
    }

    // 比较每个像素的RGB值
    for (pixel1, pixel2) in img1.pixels().zip(img2.pixels()) {
        let rgb1 = pixel1;
        let rgb2 = pixel2;
        if rgb1 != rgb2 {
            return false;
        }
    }
    true
}

pub fn cmp_image_rect(img1: &PNG, img2: &PNG, rect: &Rect) -> bool {
    // 检查图像的宽度和高度是否相同
    if img1.width() != img2.width() || img1.height() != img2.height() {
        return false;
    }

    // 比较每个像素的RGB值
    for x in rect.left..rect.left + rect.width {
        for y in rect.top..rect.top + rect.height {
            if img1.get_pixel(x as u32, y as u32) != img2.get_pixel(x as u32, y as u32) {
                return false;
            }
        }
    }
    true
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedleConfig {
    pub area: Vec<Area>,
    pub properties: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    #[serde(rename = "type")]
    pub type_field: String,
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
}

impl Into<Rect> for &Area {
    fn into(self) -> Rect {
        Rect {
            left: self.left,
            top: self.top,
            width: self.width,
            height: self.height,
        }
    }
}

#[cfg(test)]
mod test {
    use super::NeedleManager;
    use crate::needle::cmp_image_rect;
    use t_console::Rect;

    #[test]
    fn get_needle() {
        let needle = NeedleManager::new("./assets/needles");
        let (cfg, png) = needle.load_by_tag("normal2");
        let rect = &cfg.area[0];
        let rect = Rect {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
        };

        assert!(cmp_image_rect(&png, &png, &rect));

        let png2 = needle.load_file_by_tag("normal");
        assert!(!cmp_image_rect(&png, &png2, &rect));
    }
}
