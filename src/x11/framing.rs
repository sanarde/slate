// SPDX-License-Identifier: GPL-3.0-only

pub const EDGE_LEFT: u32 = 1;
pub const EDGE_RIGHT: u32 = 2;
pub const EDGE_TOP: u32 = 4;
pub const EDGE_BOTTOM: u32 = 8;
pub const CORNER_SPAN: i32 = 12;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
#[repr(C)]
pub struct Insets {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}

unsafe extern "C" {
    fn sl_frame_insets(maximized: i32, fullscreen: i32, border: i32, titlebar: i32) -> Insets;
    fn sl_frame_edges(width: i32, height: i32, x: i32, y: i32, thickness: i32, top_zone: i32, corner_span: i32) -> u32;
    fn sl_edge_cursor_glyph(edges: u32) -> u32;
    fn sl_configure_notify(out: *mut u8, window: u32, x: i32, y: i32, width: u32, height: u32, border_width: u32);
}

pub fn insets(maximized: bool, fullscreen: bool, border: i32, titlebar: i32) -> Insets {
    unsafe { sl_frame_insets(i32::from(maximized), i32::from(fullscreen), border, titlebar) }
}

pub fn edges(width: i32, height: i32, x: i32, y: i32, thickness: i32, top_zone: i32) -> u32 {
    unsafe { sl_frame_edges(width, height, x, y, thickness, top_zone, CORNER_SPAN) }
}

pub fn cursor_glyph(edges: u32) -> u32 {
    unsafe { sl_edge_cursor_glyph(edges) }
}

pub fn configure_notify(window: u32, x: i32, y: i32, width: u32, height: u32, border_width: u32) -> [u8; 32] {
    let mut event = [0u8; 32];
    unsafe { sl_configure_notify(event.as_mut_ptr(), window, x, y, width, height, border_width) };
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insets_depend_on_the_window_state() {
        assert_eq!(insets(false, false, 2, 30), Insets { left: 2, right: 2, top: 32, bottom: 2 });
        assert_eq!(insets(true, false, 2, 30), Insets { left: 0, right: 0, top: 30, bottom: 0 });
        assert_eq!(insets(false, true, 2, 30), Insets::default());
        assert_eq!(insets(true, true, 2, 30), Insets::default());
    }

    #[test]
    fn a_pointer_on_a_side_resizes_only_that_side_away_from_the_corners() {
        let (w, h, thickness, zone) = (400, 300, 2, 6);
        assert_eq!(edges(w, h, 1, 150, thickness, zone), EDGE_LEFT);
        assert_eq!(edges(w, h, 399, 150, thickness, zone), EDGE_RIGHT);
        assert_eq!(edges(w, h, 200, 299, thickness, zone), EDGE_BOTTOM);
        assert_eq!(edges(w, h, 200, 1, thickness, zone), EDGE_TOP);
    }

    #[test]
    fn corners_combine_two_edges_and_the_inside_has_none() {
        let (w, h, thickness, zone) = (400, 300, 2, 6);
        assert_eq!(edges(w, h, 1, 5, thickness, zone), EDGE_LEFT | EDGE_TOP);
        assert_eq!(edges(w, h, 1, 295, thickness, zone), EDGE_LEFT | EDGE_BOTTOM);
        assert_eq!(edges(w, h, 399, 299, thickness, zone), EDGE_RIGHT | EDGE_BOTTOM);
        assert_eq!(edges(w, h, 200, 150, thickness, zone), 0);
    }

    #[test]
    fn the_corner_span_turns_a_side_pointer_into_a_corner_near_the_end() {
        assert_eq!(edges(400, 300, 8, 1, 2, 6), EDGE_TOP | EDGE_LEFT);
        assert_eq!(edges(400, 300, 20, 1, 2, 6), EDGE_TOP);
    }

    #[test]
    fn every_resize_cursor_has_its_own_glyph() {
        assert_eq!(cursor_glyph(EDGE_LEFT | EDGE_TOP), 134);
        assert_eq!(cursor_glyph(EDGE_RIGHT | EDGE_TOP), 136);
        assert_eq!(cursor_glyph(EDGE_LEFT | EDGE_BOTTOM), 12);
        assert_eq!(cursor_glyph(EDGE_RIGHT | EDGE_BOTTOM), 14);
        assert_eq!(cursor_glyph(EDGE_LEFT), 70);
        assert_eq!(cursor_glyph(EDGE_RIGHT), 96);
        assert_eq!(cursor_glyph(EDGE_TOP), 138);
        assert_eq!(cursor_glyph(EDGE_BOTTOM), 16);
        assert_eq!(cursor_glyph(0), 16);
    }

    #[test]
    fn the_configure_notify_has_the_core_event_layout() {
        let event = configure_notify(0x0040_000c, -5, 300, 640, 480, 2);
        assert_eq!(event[0], 22);
        assert_eq!(u32::from_ne_bytes(event[4..8].try_into().unwrap()), 0x0040_000c);
        assert_eq!(u32::from_ne_bytes(event[8..12].try_into().unwrap()), 0x0040_000c);
        assert_eq!(i16::from_ne_bytes(event[16..18].try_into().unwrap()), -5);
        assert_eq!(i16::from_ne_bytes(event[18..20].try_into().unwrap()), 300);
        assert_eq!(u16::from_ne_bytes(event[20..22].try_into().unwrap()), 640);
        assert_eq!(u16::from_ne_bytes(event[22..24].try_into().unwrap()), 480);
        assert_eq!(u16::from_ne_bytes(event[24..26].try_into().unwrap()), 2);
        assert_eq!(event[26], 0);
        assert_eq!(&event[27..32], &[0, 0, 0, 0, 0]);
    }
}
