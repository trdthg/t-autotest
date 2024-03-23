use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
};

pub struct Deque<T> {
    inner: VecDeque<T>,
    max: usize,
}

impl<T> Deque<T> {
    pub fn new(max: usize) -> Self {
        Self {
            inner: VecDeque::new(),
            max,
        }
    }

    pub fn push(&mut self, elem: T) {
        if self.inner.len() == self.max {
            self.inner.pop_front();
        }
        self.inner.push_back(elem);
    }
}

impl<T> Deref for Deque<T> {
    type Target = VecDeque<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Deque<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
