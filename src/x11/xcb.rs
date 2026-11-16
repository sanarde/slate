// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::c_void;

use x11rb::connection::RequestConnection;
use x11rb::xcb_ffi::XCBConnection;

unsafe extern "C" {
    fn sl_shape_clear(connection: *mut c_void, frame: u32) -> i32;
    fn sl_shape_rounded(
        connection: *mut c_void,
        frame: u32,
        argb: *const u8,
        length: usize,
        width: i32,
        height: i32,
        top: i32,
        radius: i32,
    ) -> i32;
    fn sl_put_image_rows(
        connection: *mut c_void,
        drawable: u32,
        gc: u32,
        width: u16,
        height: u16,
        depth: u8,
        max_request_bytes: usize,
        data: *const u8,
    ) -> i32;
}

pub fn shape_clear(conn: &XCBConnection, frame: u32) -> Result<(), String> {
    let status = unsafe { sl_shape_clear(conn.get_raw_xcb_connection(), frame) };
    if status == 0 { Ok(()) } else { Err("the X connection failed while clearing the shape".to_string()) }
}

pub fn shape_rounded(
    conn: &XCBConnection,
    frame: u32,
    argb: &[u8],
    size: [i32; 4],
) -> Result<(), String> {
    let [width, height, top, radius] = size;
    let status = unsafe {
        sl_shape_rounded(
            conn.get_raw_xcb_connection(),
            frame,
            argb.as_ptr(),
            argb.len(),
            width,
            height,
            top,
            radius,
        )
    };
    if status == 0 { Ok(()) } else { Err("the rounded shape could not be set".to_string()) }
}

pub fn put_image(
    conn: &XCBConnection,
    drawable: u32,
    gc: u32,
    width: u16,
    height: u16,
    depth: u8,
    data: &[u8],
) -> Result<(), String> {
    if data.len() != width as usize * height as usize * 4 {
        return Err("the image does not match its size".to_string());
    }
    let status = unsafe {
        sl_put_image_rows(
            conn.get_raw_xcb_connection(),
            drawable,
            gc,
            width,
            height,
            depth,
            conn.maximum_request_bytes(),
            data.as_ptr(),
        )
    };
    if status == 0 { Ok(()) } else { Err("the X connection failed while sending an image".to_string()) }
}

#[cfg(test)]
mod tests {
    #[derive(Clone, Copy, Default, PartialEq, Debug)]
    #[repr(C)]
    struct Rectangle {
        x: i16,
        y: i16,
        width: u16,
        height: u16,
    }

    unsafe extern "C" {
        fn sl_rows_per_request(max_request_bytes: usize, row_bytes: usize, height: usize) -> usize;
        fn sl_shape_rectangles(
            argb: *const u8,
            length: usize,
            width: i32,
            height: i32,
            top: i32,
            radius: i32,
            out: *mut Rectangle,
            capacity: usize,
        ) -> usize;
    }

    fn rectangles(argb: &[u8], width: i32, height: i32, top: i32, radius: i32) -> Vec<Rectangle> {
        let capacity = (radius + 2).max(2) as usize;
        let mut out = vec![Rectangle::default(); capacity];
        let count = unsafe { sl_shape_rectangles(argb.as_ptr(), argb.len(), width, height, top, radius, out.as_mut_ptr(), capacity) };
        out.truncate(count);
        out
    }

    fn rows(alphas: &[&[u8]]) -> Vec<u8> {
        alphas.iter().flat_map(|row| row.iter().flat_map(|alpha| [0, 0, 0, *alpha])).collect()
    }

    #[test]
    fn rounded_rows_become_one_pixel_rectangles_above_the_body() {
        let argb = rows(&[&[0, 0, 255, 255, 0, 0], &[0, 255, 255, 255, 255, 0], &[255; 6]]);
        assert_eq!(
            rectangles(&argb, 6, 10, 3, 2),
            vec![
                Rectangle { x: 2, y: 0, width: 2, height: 1 },
                Rectangle { x: 1, y: 1, width: 4, height: 1 },
                Rectangle { x: 0, y: 2, width: 6, height: 1 },
                Rectangle { x: 0, y: 3, width: 6, height: 7 },
            ]
        );
    }

    #[test]
    fn the_corner_rows_are_limited_by_the_titlebar_height() {
        let argb = rows(&[&[255; 4]]);
        let result = rectangles(&argb, 4, 6, 1, 8);
        assert_eq!(result.first(), Some(&Rectangle { x: 0, y: 0, width: 4, height: 1 }));
        assert_eq!(result.last(), Some(&Rectangle { x: 0, y: 1, width: 4, height: 5 }));
    }

    #[test]
    fn a_window_no_taller_than_its_corner_rows_has_no_body_rectangle() {
        let argb = rows(&[&[255; 3], &[255; 3]]);
        let result = rectangles(&argb, 3, 2, 5, 4);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|rectangle| rectangle.height == 1));
    }

    #[test]
    fn bad_sizes_give_no_rectangles() {
        let argb = rows(&[&[255; 3]]);
        assert!(rectangles(&argb, 0, 5, 1, 2).is_empty());
        assert!(rectangles(&argb, 70000, 5, 1, 2).is_empty());
        assert!(rectangles(&argb, 3, 0, 1, 2).is_empty());
    }

    fn rows_per_request(max_request_bytes: usize, row_bytes: usize, height: usize) -> usize {
        unsafe { sl_rows_per_request(max_request_bytes, row_bytes, height) }
    }

    #[test]
    fn a_large_request_budget_sends_everything_at_once() {
        assert_eq!(rows_per_request(16 * 1024 * 1024, 5120, 40), 40);
    }

    #[test]
    fn a_small_budget_splits_the_image_into_the_rows_that_fit() {
        assert_eq!(rows_per_request(10_000 + 64, 1000, 40), 10);
        assert_eq!(rows_per_request(64 + 1000, 1000, 40), 1);
    }

    #[test]
    fn a_budget_smaller_than_a_row_still_sends_one_row_at_a_time() {
        assert_eq!(rows_per_request(100, 1000, 7), 1);
        assert_eq!(rows_per_request(0, 1000, 7), 1);
    }

    #[test]
    fn empty_images_have_nothing_to_send() {
        assert_eq!(rows_per_request(1_000_000, 0, 10), 0);
        assert_eq!(rows_per_request(1_000_000, 100, 0), 0);
    }
}
