impl From<(u16, u16, u16, u16)> for Rect {
    fn from(val: (u16, u16, u16, u16)) -> Self {
        Self {
            left: val.0,
            top: val.1,
            width: val.2,
            height: val.3,
        }
    }
}

#[derive(Clone)]
pub struct Rect {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
}

impl From<vnc::Rect> for Rect {
    fn from(value: vnc::Rect) -> Self {
        Self {
            left: value.left,
            top: value.top,
            width: value.width,
            height: value.height,
        }
    }
}

impl Into<vnc::Rect> for &Rect {
    fn into(self) -> vnc::Rect {
        vnc::Rect {
            left: self.left,
            top: self.top,
            width: self.width,
            height: self.height,
        }
    }
}

// data rect
#[derive(Clone)]
pub struct RectContainer<P> {
    pub rect: Rect,
    pub data: Vec<P>,
}

impl<P: Clone> RectContainer<P> {
    pub fn new(rect: Rect) -> Self {
        let mut data = Vec::with_capacity(rect.width as usize * rect.height as usize);
        unsafe { data.set_len(rect.width as usize * rect.height as usize) };
        Self { rect, data }
    }

    pub fn new_with_data(rect: Rect, data: Vec<P>) -> Self {
        Self { rect, data }
    }

    pub fn get_rect(&self, left: u16, top: u16, width: u16, height: u16) -> Vec<P> {
        let mut data = Vec::with_capacity(width as usize * height as usize);
        for col in left..left + self.rect.width {
            for row in top..top + self.rect.height {
                let p = self.get(row as usize, col as usize);
                data.push(p);
            }
        }
        data
    }

    pub fn get(&self, row: usize, col: usize) -> P {
        assert!(row < self.rect.height as usize && col < self.rect.width as usize);
        self.data[row * self.rect.width as usize + col].clone()
    }

    pub fn set(&mut self, row: usize, col: usize, p: P) {
        assert!(row < self.rect.height as usize && col < self.rect.width as usize);
        self.data[row * self.rect.width as usize + col] = p
    }

    pub fn update(&mut self, rect: RectContainer<P>) {
        let offset_left = rect.rect.left - self.rect.left;
        let offset_top = rect.rect.top - self.rect.top;

        for col in 0..rect.rect.width {
            for row in 0..rect.rect.height {
                self.set(
                    (row + offset_top) as usize,
                    (col + offset_left) as usize,
                    rect.get(row as usize, col as usize),
                )
            }
        }
    }
}

#[cfg(test)]
mod test {

    use super::*;

    type U8Screen = RectContainer<u8>;

    #[test]
    fn test_update() {
        let mut sc = U8Screen::new_with_data(
            (0, 0, 3, 3).into(),
            vec![
                1, 2, 3, //
                4, 5, 6, //
                7, 8, 9, //
            ],
        );

        assert_eq!(sc.get(1, 2), 6);

        let sub_sc = U8Screen::new_with_data(
            (1, 1, 2, 2).into(),
            vec![
                1, 2, //
                3, 4, //
            ],
        );
        assert_eq!(sc.get(0, 1), 2);

        sc.update(sub_sc);
        assert_eq!(sc.get(1, 2), 2);
    }
}
