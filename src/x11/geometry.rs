// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Strut {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

unsafe extern "C" {
    fn sl_rect_contains(rect: *const Rect, x: i32, y: i32) -> i32;
    fn sl_rect_union(a: *const Rect, b: *const Rect) -> Rect;
    fn sl_rect_work_area(monitor: *const Rect, bounds: *const Rect, strut: *const Strut) -> Rect;
    fn sl_monitor_index(monitors: *const Rect, count: usize, rect: *const Rect) -> usize;
}

impl Rect {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        unsafe { sl_rect_contains(self, x, y) != 0 }
    }

    pub fn union(&self, other: &Rect) -> Rect {
        unsafe { sl_rect_union(self, other) }
    }

    pub fn work_area(&self, bounds: &Rect, strut: &Strut) -> Rect {
        unsafe { sl_rect_work_area(self, bounds, strut) }
    }
}

pub fn monitor_index(monitors: &[Rect], rect: &Rect) -> usize {
    unsafe { sl_monitor_index(monitors.as_ptr(), monitors.len(), rect) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_contain_their_top_left_but_not_the_bottom_right() {
        let rect = Rect::new(10, 20, 100, 50);
        assert!(rect.contains(10, 20));
        assert!(rect.contains(109, 69));
        assert!(!rect.contains(110, 40));
        assert!(!rect.contains(50, 70));
        assert!(!rect.contains(9, 20));
    }

    #[test]
    fn the_union_covers_both_rectangles() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(-50, 80, 60, 100);
        assert_eq!(a.union(&b), Rect::new(-50, 0, 150, 180));
    }

    #[test]
    fn monitors_that_only_touch_a_rectangle_do_not_count_as_overlapping_it() {
        let monitors = [Rect::new(0, 0, 100, 100), Rect::new(100, 0, 100, 100)];
        assert_eq!(monitor_index(&monitors, &Rect::new(90, 10, 20, 20)), 1);
        assert_eq!(monitor_index(&monitors, &Rect::new(80, 10, 20, 20)), 0);
        assert_eq!(monitor_index(&monitors, &Rect::new(500, 500, 10, 10)), 1);
    }

    #[test]
    fn struts_only_shrink_the_monitors_on_the_edge_of_the_bounds() {
        let left = Rect::new(0, 0, 1000, 800);
        let right = Rect::new(1000, 0, 1000, 800);
        let bounds = left.union(&right);
        let strut = Strut {
            left: 50,
            right: 0,
            top: 30,
            bottom: 0,
        };

        assert_eq!(left.work_area(&bounds, &strut), Rect::new(50, 30, 950, 770));
        assert_eq!(right.work_area(&bounds, &strut), Rect::new(1000, 30, 1000, 770));
    }

    #[test]
    fn a_strut_larger_than_the_monitor_leaves_nothing_instead_of_wrapping() {
        let monitor = Rect::new(0, 0, 100, 100);
        let strut = Strut {
            left: 500,
            right: 500,
            top: 500,
            bottom: 500,
        };
        let area = monitor.work_area(&monitor, &strut);
        assert_eq!((area.w, area.h), (0, 0));
    }

    #[test]
    fn a_rectangle_belongs_to_the_monitor_it_overlaps_most() {
        let monitors = [Rect::new(0, 0, 1000, 800), Rect::new(1000, 0, 1000, 800)];
        assert_eq!(monitor_index(&monitors, &Rect::new(900, 100, 300, 200)), 1);
        assert_eq!(monitor_index(&monitors, &Rect::new(800, 100, 300, 200)), 0);
        assert_eq!(monitor_index(&[], &Rect::new(0, 0, 10, 10)), 0);
    }
}
