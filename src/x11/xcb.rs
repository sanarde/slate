// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::c_void;

use x11rb::connection::RequestConnection;
use x11rb::xcb_ffi::XCBConnection;

unsafe extern "C" {
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
    unsafe extern "C" {
        fn sl_rows_per_request(max_request_bytes: usize, row_bytes: usize, height: usize) -> usize;
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
