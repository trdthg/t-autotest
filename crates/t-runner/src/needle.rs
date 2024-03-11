use std::{fs::File, io::BufReader, path::PathBuf};

use serde::{Deserialize, Serialize};
use t_console::{Rect, PNG};

pub struct NeedleManager {
    dir: PathBuf,
}

impl NeedleManager {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn load_by_tag(&self, tag: &str) -> Option<(NeedleConfig, PNG)> {
        let needle_png = self.load_file_by_tag(tag);
        let json_file = File::open(self.dir.join(format!("{tag}.json"))).ok()?;
        let json: NeedleConfig = serde_json::from_reader(BufReader::new(json_file)).ok()?;
        Some((json, needle_png?))
    }

    pub fn load_file_by_tag(&self, tag: &str) -> Option<PNG> {
        let needle_file = File::open(self.dir.join(format!("{tag}.png"))).ok()?;
        let needle_png = image::load(BufReader::new(needle_file), image::ImageFormat::Png).ok()?;
        Some(needle_png.into_rgb8())
    }

    pub fn cmp_by_tag(&self, s: &PNG, tag: &str) -> Option<bool> {
        let Some((needle_cfg, needle_png)) = self.load_by_tag(tag) else {
            return None;
        };
        for area in needle_cfg.area.iter() {
            if !cmp_image_rect(&needle_png, s, &area.into()) {
                return Some(false);
            }
        }
        Some(true)
    }
}

#[allow(dead_code)]
pub fn cmp_image_full(img1: &PNG, img2: &PNG) -> bool {
    // 检查图像的宽度和高度是否相同
    if img1.width() != img2.width() || img1.height() != img2.height() {
        return false;
    }

    // 比较每个像素的 RGB 值
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

    // 比较每个像素的 RGB 值
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

impl From<&Area> for Rect {
    fn from(val: &Area) -> Self {
        Rect {
            left: val.left,
            top: val.top,
            width: val.width,
            height: val.height,
        }
    }
}

#[cfg(test)]
mod test {
    use std::fs;

    use super::NeedleManager;
    use crate::needle::{cmp_image_rect, Area, NeedleConfig};
    use image::{ImageBuffer, Rgb};
    use t_console::Rect;

    fn init_needle_manager() -> NeedleManager {
        // 创建临时文件夹
        let temp_dir = std::env::temp_dir();
        println!("{:?}", temp_dir);

        let tmp_needle_folder = temp_dir.join("needle");
        if fs::metadata(&tmp_needle_folder).is_ok() {
            fs::remove_dir_all(&tmp_needle_folder).unwrap();
        }
        fs::create_dir(&tmp_needle_folder).unwrap();
        let mut image_buffer: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(5, 5);
        for pixel in image_buffer.pixels_mut() {
            *pixel = Rgb([0, 0, 0]);
        }

        image_buffer
            .save_with_format(
                tmp_needle_folder.join("output.png"),
                image::ImageFormat::Png,
            )
            .unwrap();
        fs::write(
            tmp_needle_folder.join("output.json"),
            r#"
            {
                "area": [
                    {
                        "type": "match",
                        "left": 0,
                        "top": 0,
                        "width": 5,
                        "height": 5
                    }
                ],
                "properties": [],
                "tags": [
                    "output"
                ]
            }
        "#,
        )
        .unwrap();

        // update middle pixel
        image_buffer.put_pixel(2, 2, Rgb([255, 255, 255]));
        image_buffer
            .save_with_format(
                tmp_needle_folder.join("output2.png"),
                image::ImageFormat::Png,
            )
            .unwrap();
        fs::write(
            tmp_needle_folder.join("output2.json"),
            r#"
            {
                "area": [
                    {
                        "type": "match",
                        "left": 0,
                        "top": 0,
                        "width": 5,
                        "height": 5
                    }
                ],
                "properties": [],
                "tags": [
                    "output2"
                ]
            }
        "#,
        )
        .unwrap();
        NeedleManager::new(tmp_needle_folder)
    }

    #[test]
    fn get_needle() {
        let needle = init_needle_manager();
        let (cfg, png) = needle.load_by_tag("output").unwrap();

        assert_eq!(
            cfg,
            NeedleConfig {
                area: vec![Area {
                    type_field: "match".to_string(),
                    left: 0,
                    top: 0,
                    width: 5,
                    height: 5,
                }],
                properties: Vec::new(),
                tags: vec!["output".to_string()]
            }
        );

        let rect = &cfg.area[0];
        let rect = Rect {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
        };
        assert!(cmp_image_rect(&png, &png, &rect));

        let png2 = needle.load_file_by_tag("output2").unwrap();
        assert!(!cmp_image_rect(&png, &png2, &rect));
    }
}
