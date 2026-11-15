// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Default)]
pub struct Strut {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h as i32
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect {
            x,
            y,
            w: (self.right().max(other.right()) - x) as u32,
            h: (self.bottom().max(other.bottom()) - y) as u32,
        }
    }

    pub fn overlap_area(&self, other: &Rect) -> i64 {
        let width = self.right().min(other.right()) - self.x.max(other.x);
        let height = self.bottom().min(other.bottom()) - self.y.max(other.y);
        if width <= 0 || height <= 0 {
            return 0;
        }
        width as i64 * height as i64
    }

    pub fn work_area(&self, bounds: &Rect, strut: &Strut) -> Rect {
        let mut area = *self;

        if self.x == bounds.x {
            area.x += strut.left as i32;
            area.w = area.w.saturating_sub(strut.left);
        }
        if self.right() == bounds.right() {
            area.w = area.w.saturating_sub(strut.right);
        }
        if self.y == bounds.y {
            area.y += strut.top as i32;
            area.h = area.h.saturating_sub(strut.top);
        }
        if self.bottom() == bounds.bottom() {
            area.h = area.h.saturating_sub(strut.bottom);
        }
        area
    }
}
