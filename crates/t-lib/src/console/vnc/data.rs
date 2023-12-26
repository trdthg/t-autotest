struct IteratorAcc {
    curr: usize,
}

pub struct FullScreen<Pixel> {
    inner: Rect<Pixel>,
}

impl<Pixel> FullScreen<Pixel> {
    pub fn new(width: usize, height: usize) -> Self {
        Self { inner: todo!() }
    }
}

struct Rect<Pixel> {
    width: usize,
    height: usize,
    data: Vec<Pixel>,
    acc: IteratorAcc,
}

impl<'a, Pixel> Iterator for &'a Rect<Pixel> {
    type Item = &'a Pixel;

    fn next(&mut self) -> Option<Self::Item> {
        if self.acc.curr == self.width * self.height {
            return None;
        }
        return self.data.get(self.acc.curr);
    }
}

// data rect
struct ScreenRect<Pixel> {
    left: usize,
    top: usize,
    data: Rect<Pixel>,
}

impl<'a, Pixel> Iterator for &'a ScreenRect<Pixel> {
    type Item = &'a Pixel;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.acc.curr == self.data.width * self.data.height {
            return None;
        }
        return self.data.data.get(self.data.acc.curr);
    }
}

impl<Pixel: Clone> Rect<Pixel> {
    pub fn new(x: usize, y: usize) -> Self {
        Self {
            width: x,
            height: y,
            data: Vec::with_capacity(x * y),
            acc: IteratorAcc { curr: 0 },
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Pixel {
        assert!(x < self.width && y < self.height);
        return self.data[x * self.width + y].clone();
    }

    pub fn set(&mut self, x: usize, y: usize, p: Pixel) {
        assert!(x < self.width && y < self.height);
        self.data[x * self.width + y] = p
    }

    pub fn update(&mut self, rect: ScreenRect<Pixel>) {
        for x in rect.left..(rect.left + rect.data.width) {
            for y in rect.top..(rect.top + rect.data.width) {
                self.set(x, y, rect.data.get(x, y))
            }
        }
    }
}
