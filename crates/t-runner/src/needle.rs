use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use t_console::{Rect, PNG};

pub struct Needle {
    pub config: NeedleConfig,
    pub data: PNG,
}

pub struct NeedleManager {
    dir: PathBuf,
}

impl NeedleManager {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn load_by_tag(&self, tag: &str) -> Option<Needle> {
        let needle_png = self.load_file_by_tag(tag);
        let json_file = File::open(self.dir.join(format!("{tag}.json"))).ok()?;
        let json: NeedleConfig = serde_json::from_reader(BufReader::new(json_file)).ok()?;
        Some(Needle {
            config: json,
            data: needle_png?,
        })
    }

    pub fn load_file_by_tag(&self, tag: &str) -> Option<PNG> {
        let needle_file = File::open(self.dir.join(format!("{tag}.png"))).ok()?;
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

    pub fn cmp_by_tag(&self, s: &PNG, tag: &str) -> Option<bool> {
        let Some(needle) = self.load_by_tag(tag) else {
            return None;
        };
        for area in needle.config.areas.iter() {
            if !needle.data.cmp_rect(s, &area.into()) {
                return Some(false);
            }
        }
        Some(true)
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
        let png = needle_mg.load_by_tag("output").unwrap();

        assert_eq!(
            png.config,
            NeedleConfig {
                areas: vec![Area {
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

        let rect = &png.config.areas[0];
        let rect = Rect {
            left: rect.left,
            top: rect.top,
            width: rect.width,
            height: rect.height,
        };
        assert!(png.data.cmp_rect(&png.data, &rect));

        let png2 = needle_mg.load_file_by_tag("output2").unwrap();
        assert!(png.data.cmp_rect(&png2, &rect));
    }
}
