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
    fn sl_strut_parse(values: *const u32, count: usize, strut: *mut Strut) -> i32;
    fn sl_strut_merge(into: *mut Strut, other: *const Strut);
    fn sl_bounds(monitors: *const Rect, count: usize, fallback: *const Rect) -> Rect;
    fn sl_work_areas(monitors: *const Rect, count: usize, bounds: *const Rect, strut: *const Strut, areas: *mut Rect);
    fn sl_monitor_index(monitors: *const Rect, count: usize, rect: *const Rect) -> usize;
}

impl Rect {
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        unsafe { sl_rect_contains(self, x, y) != 0 }
    }

}

impl Strut {
    pub fn parse(values: &[u32]) -> Option<Strut> {
        let mut strut = Strut::default();
        (unsafe { sl_strut_parse(values.as_ptr(), values.len(), &mut strut) } != 0).then_some(strut)
    }

    pub fn merge(&mut self, other: &Strut) {
        unsafe { sl_strut_merge(self, other) }
    }
}

pub fn bounds(monitors: &[Rect], fallback: &Rect) -> Rect {
    unsafe { sl_bounds(monitors.as_ptr(), monitors.len(), fallback) }
}

pub fn work_areas(monitors: &[Rect], bounds: &Rect, strut: &Strut) -> Vec<Rect> {
    let mut areas = vec![Rect::default(); monitors.len()];
    unsafe { sl_work_areas(monitors.as_ptr(), monitors.len(), bounds, strut, areas.as_mut_ptr()) };
    areas
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
    fn the_bounds_of_overlapping_and_negative_monitors_cover_all_of_them() {
        let monitors = [Rect::new(0, 0, 100, 100), Rect::new(-50, 80, 60, 100)];
        assert_eq!(bounds(&monitors, &Rect::default()), Rect::new(-50, 0, 150, 180));
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
        let monitors = [Rect::new(0, 0, 1000, 800), Rect::new(1000, 0, 1000, 800)];
        let strut = Strut {
            left: 50,
            right: 0,
            top: 30,
            bottom: 0,
        };

        let areas = work_areas(&monitors, &bounds(&monitors, &Rect::default()), &strut);
        assert_eq!(areas, vec![Rect::new(50, 30, 950, 770), Rect::new(1000, 30, 1000, 770)]);
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
        let areas = work_areas(&[monitor], &monitor, &strut);
        assert_eq!((areas[0].w, areas[0].h), (0, 0));
    }

    #[test]
    fn struts_come_from_the_first_four_values_and_need_at_least_four() {
        let partial = [10, 20, 30, 40, 0, 0, 0, 0, 0, 0, 0, 0];
        let strut = Strut::parse(&partial).unwrap();
        assert_eq!((strut.left, strut.right, strut.top, strut.bottom), (10, 20, 30, 40));
        assert!(Strut::parse(&[1, 2, 3]).is_none());
        assert!(Strut::parse(&[]).is_none());
    }

    #[test]
    fn merged_struts_keep_the_largest_side_of_each_panel() {
        let mut strut = Strut {
            left: 10,
            right: 0,
            top: 40,
            bottom: 0,
        };
        strut.merge(&Strut {
            left: 5,
            right: 30,
            top: 20,
            bottom: 0,
        });
        assert_eq!((strut.left, strut.right, strut.top, strut.bottom), (10, 30, 40, 0));
    }

    #[test]
    fn the_bounds_cover_every_monitor_and_fall_back_when_there_are_none() {
        let screen = Rect::new(0, 0, 640, 480);
        assert_eq!(bounds(&[], &screen), screen);
        let monitors = [Rect::new(0, 0, 1000, 800), Rect::new(1000, 100, 500, 600)];
        assert_eq!(bounds(&monitors, &screen), Rect::new(0, 0, 1500, 800));
    }

    #[test]
    fn work_areas_follow_the_monitors_in_order() {
        let monitors = [Rect::new(0, 0, 1000, 800), Rect::new(1000, 0, 1000, 800)];
        let strut = Strut {
            left: 0,
            right: 40,
            top: 0,
            bottom: 24,
        };
        let areas = work_areas(&monitors, &bounds(&monitors, &Rect::default()), &strut);
        assert_eq!(areas, vec![Rect::new(0, 0, 1000, 776), Rect::new(1000, 0, 960, 776)]);
    }

    #[test]
    fn a_rectangle_belongs_to_the_monitor_it_overlaps_most() {
        let monitors = [Rect::new(0, 0, 1000, 800), Rect::new(1000, 0, 1000, 800)];
        assert_eq!(monitor_index(&monitors, &Rect::new(900, 100, 300, 200)), 1);
        assert_eq!(monitor_index(&monitors, &Rect::new(800, 100, 300, 200)), 0);
        assert_eq!(monitor_index(&[], &Rect::new(0, 0, 10, 10)), 0);
    }
}
