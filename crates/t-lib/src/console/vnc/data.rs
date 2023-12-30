// data rect
pub struct RectContainer<P> {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub data: Vec<P>,
}

impl<P: Clone> RectContainer<P> {
    pub fn new(left: u16, top: u16, width: u16, height: u16) -> Self {
        let mut data = Vec::with_capacity(width as usize * height as usize);
        unsafe { data.set_len(width as usize * height as usize) };
        Self {
            left,
            top,
            width,
            height,
            data,
        }
    }

    pub fn copy(&self, left: u16, top: u16, width: u16, height: u16) -> Vec<P> {
        let mut data = Vec::with_capacity(width as usize * height as usize);
        for col in left..left + self.width {
            for row in top..top + self.height {
                let p = self.get(row as usize, col as usize);
                data.push(p);
            }
        }
        data
    }

    pub fn get(&self, row: usize, col: usize) -> P {
        assert!(row < self.height as usize && col < self.width as usize);
        self.data[row * self.width as usize + col].clone()
    }

    pub fn set(&mut self, row: usize, col: usize, p: P) {
        assert!(row < self.height as usize && col < self.width as usize);
        self.data[row * self.width as usize + col] = p
    }

    pub fn update(&mut self, rect: RectContainer<P>) {
        let offset_left = rect.left - self.left;
        let offset_top = rect.top - self.top;

        for col in 0..rect.width {
            for row in 0..rect.height {
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
        let mut sc = U8Screen::new(0, 0, 3, 3);
        sc.data = vec![
            1, 2, 3, //
            4, 5, 6, //
            7, 8, 9, //
        ];

        assert_eq!(sc.get(1, 2), 6);

        let mut sub_sc = U8Screen::new(1, 1, 2, 2);
        sub_sc.data = vec![
            1, 2, //
            3, 4, //
        ];
        assert_eq!(sc.get(0, 1), 2);

        sc.update(sub_sc);
        assert_eq!(sc.get(1, 2), 2);
    }
}
