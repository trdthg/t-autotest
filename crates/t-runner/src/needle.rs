use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use t_console::{Rect, PNG};
use tracing::{info, warn};

pub struct Needle {
    pub config: NeedleConfig,
    pub data: PNG,
}

impl Needle {
    pub fn cmp(s: &PNG, needle: &Needle, min_same: Option<f32>) -> (f32, bool) {
        if needle.config.areas.is_empty() {
            warn!("this needle has no match ares");
            return (1.0, true);
        }

        let mut not_same = 0;
        let mut all = 0;
        for area in needle.config.areas.iter() {
            all += area.width * area.height;
            let count = s.cmp_rect_and_count(&needle.data, &area.into());
            not_same += count;
        }

        let res = 1. - (not_same as f32 / all as f32);
        info!(res = res, all = all, not_same = not_same);
        (res, res >= min_same.unwrap_or(0.95))
    }
}

pub struct NeedleManager {
    dir: PathBuf,
}

impl NeedleManager {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_path_buf(),
        }
    }

    pub fn load(&self, tag: &str) -> Option<Needle> {
        let needle_png = self.load_image(self.dir.join(format!("{}.png", tag)))?;
        let json: NeedleConfig = self.load_json(self.dir.join(format!("{}.json", tag)))?;
        Some(Needle {
            config: json,
            data: needle_png,
        })
    }

    pub fn load_image(&self, tag: impl AsRef<Path>) -> Option<PNG> {
        let needle_file = File::open(tag).ok()?;
        let needle_png = image::load(BufReader::new(needle_file), image::ImageFormat::Png).ok()?;
        match needle_png {
            image::DynamicImage::ImageRgb8(img) => {
                let data = img.bytes();
                let data = data.map(|x| x.unwrap()).collect::<Vec<u8>>();
                Some(PNG::new_with_data(
                    img.width() as u16,
                    img.height() as u16,
                    data,
                    3,
                ))
            }
            _ => None,
        }
    }

    pub fn load_json(&self, tag: impl AsRef<Path>) -> Option<NeedleConfig> {
        let json_file = File::open(tag).ok()?;
        let json: NeedleConfig = serde_json::from_reader(BufReader::new(json_file)).ok()?;
        Some(json)
    }

    pub fn cmp(&self, s: &PNG, filename: &str, min_same: Option<f32>) -> Option<(f32, bool)> {
        let needle = self.load(filename)?;
        Some(Needle::cmp(s, &needle, min_same))
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedleConfig {
    pub areas: Vec<Area>,
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
    pub click: Option<AreaClick>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaClick {
    pub left: u16,
    pub top: u16,
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
    use crate::needle::{Area, NeedleConfig};
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
        let needle_mg = init_needle_manager();
        let png = needle_mg.load("output").unwrap();

        assert_eq!(
            png.config,
            NeedleConfig {
                areas: vec![Area {
                    type_field: "match".to_string(),
                    left: 0,
                    top: 0,
                    width: 5,
                    height: 5,
                    click: None,
                }],
                properties: Vec::new(),
                tags: vec!["output".to_string()]
            }
        );

        let rect = &png.config.areas[0];
        let rect = Rect {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
        };
        assert!(png.data.cmp_rect(&png.data, &rect));

        let png2 = needle_mg.load_image("output2").unwrap();
        assert!(png.data.cmp_rect(&png2, &rect));
    }
}
