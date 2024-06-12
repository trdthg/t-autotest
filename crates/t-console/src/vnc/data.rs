use image::{DynamicImage, RgbImage};

pub type Rect = t_vnc::Rect;

// data rect
#[derive(Clone, Debug)]
pub struct Container {
    pub width: u16,
    pub height: u16,
    pub data: Vec<u8>,
    pub pixel_size: usize,
}

impl Container {
    pub fn new(width: u16, height: u16, pixel_size: usize) -> Self {
        let cap = width as usize * height as usize * pixel_size;
        let data = vec![0; cap];
        Self {
            width,
            height,
            data,
            pixel_size,
        }
    }

    pub fn new_with_data(width: u16, height: u16, data: Vec<u8>, pixel_size: usize) -> Self {
        Self {
            width,
            height,
            data,
            pixel_size,
        }
    }

    fn get_pixel_start(&self, row: u16, col: u16) -> usize {
        (row as usize * self.width as usize + col as usize) * self.pixel_size
    }

    pub fn set_zero(&mut self) {
        self.data.iter_mut().for_each(|x| *x = 0);
    }

    pub fn get(&self, row: u16, col: u16) -> &[u8] {
        assert!(row < self.height && col < self.width);
        let start = self.get_pixel_start(row, col);
        &self.data[start..(start + self.pixel_size)]
    }

    pub fn set(&mut self, row: u16, col: u16, p: &[u8]) {
        assert!(row < self.height && col < self.width);
        assert!(p.len() == self.pixel_size);
        let start = self.get_pixel_start(row, col);
        self.data[start..(start + self.pixel_size)]
            .copy_from_slice(&p[..(start + self.pixel_size - start)]);
    }

    pub fn get_rect(&self, r: Rect) -> Vec<u8> {
        let mut data = Vec::with_capacity((r.width * r.height) as usize * self.pixel_size);
        for col in r.left..r.left + r.width {
            for row in r.top..r.top + r.height {
                let p = self.get(row, col);
                data.extend(p);
            }
        }
        data
    }

    pub fn set_rect(&mut self, left: u16, top: u16, c: &Container) {
        assert!(c.pixel_size == self.pixel_size);
        for row in 0..(if self.height - top > c.height {
            c.height
        } else {
            self.height - top
        }) {
            for col in 0..(if self.width - left > c.width {
                c.width
            } else {
                self.width - left
            }) {
                self.set(row + top, col + left, c.get(row, col));
            }
        }
    }

    pub fn into_img(self) -> DynamicImage {
        DynamicImage::ImageRgb8(
            RgbImage::from_vec(self.width as u32, self.height as u32, self.data).unwrap(),
        )
    }

    pub fn as_img(&self) -> DynamicImage {
        DynamicImage::ImageRgb8(
            RgbImage::from_vec(self.width as u32, self.height as u32, self.data.clone()).unwrap(),
        )
    }

    pub fn cmp(&self, o: &Self) -> bool {
        // check width and height
        if self.width != o.width || self.height != o.height {
            return false;
        }

        // compare rgb
        for (pixel1, pixel2) in self.data.iter().zip(&o.data) {
            let rgb1 = pixel1;
            let rgb2 = pixel2;
            if rgb1 != rgb2 {
                return false;
            }
        }
        true
    }

    pub fn cmp_rect(&self, o: &Self, rect: &Rect) -> bool {
        // check width and height
        if self.width != o.width || self.height != o.height {
            return false;
        }

        // compare rgb
        for row in rect.top..rect.top + rect.height {
            for col in rect.left..rect.left + rect.width {
                if let Some((p1, p2)) = self.get(row, col).iter().zip(o.get(row, col)).next() {
                    return *p1 == *p2;
                }
            }
        }
        true
    }

    pub fn cmp_rect_and_count(&self, o: &Self, rect: &Rect) -> i32 {
        // check width and height
        if self.width != o.width || self.height != o.height {
            return rect.width as i32 * rect.height as i32;
        }

        let mut n = 0;

        for row in rect.top..rect.top + rect.height {
            for col in rect.left..rect.left + rect.width {
                let p1 = self.get(row, col);
                let p2 = o.get(row, col);
                for i in 0..self.pixel_size {
                    if p1[i] != p2[i] {
                        n += 1;
                        break;
                    }
                }
            }
        }
        n
    }
}

#[cfg(test)]
mod test {

    use t_vnc::Rect;

    use super::*;

    #[test]
    fn test_update() {
        let mut sc = Container::new_with_data(
            3,
            3,
            vec![
                1, 2, 3, //
                4, 5, 6, //
                7, 8, 9, //
            ],
            1,
        );

        assert_eq!(sc.get(1, 2), vec![6]);

        let sub_sc = Container::new_with_data(
            2,
            2,
            vec![
                1, 2, //
                3, 4, //
            ],
            1,
        );
        assert_eq!(sc.get(0, 1), vec![2]);

        sc.set_rect(1, 1, &sub_sc);

        assert_eq!(sc.get(1, 2), vec![2]);
    }

    #[test]
    fn test_update2() {
        let mut sc = Container::new_with_data(
            3,
            3,
            vec![
                1, 1, 2, 2, 3, 3, //
                4, 4, 5, 5, 6, 6, //
                7, 7, 8, 8, 9, 9, //
            ],
            2,
        );

        assert_eq!(sc.get(1, 2), vec![6, 6]);

        let sub_sc = Container::new_with_data(
            2,
            2,
            vec![
                1, 1, 2, 2, //
                3, 3, 4, 4, //
            ],
            2,
        );
        assert_eq!(sc.get(0, 1), vec![2, 2]);

        sc.set_rect(1, 1, &sub_sc);

        assert!(sc.cmp_rect(
            &Container::new_with_data(
                3,
                3,
                vec![
                    1, 1, 2, 2, 3, 3, //
                    4, 4, 1, 1, 2, 2, //
                    7, 7, 3, 3, 4, 4, //
                ],
                2,
            ),
            &Rect {
                left: 0,
                top: 0,
                width: 3,
                height: 3,
            },
        ));
    }
}
