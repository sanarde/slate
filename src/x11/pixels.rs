// SPDX-License-Identifier: GPL-3.0-only

unsafe extern "C" {
    fn sl_composite_over(argb: *const u8, pixels: usize, background: u32, out: *mut u8);
}

pub fn composite_over(argb: &[u8], background: u32) -> Vec<u8> {
    let pixels = argb.len() / 4;
    let mut out = vec![0u8; pixels * 4];
    unsafe { sl_composite_over(argb.as_ptr(), pixels, background, out.as_mut_ptr()) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Default, PartialEq, Debug)]
    #[repr(C)]
    struct Span {
        row: i32,
        first: i32,
        last: i32,
    }

    unsafe extern "C" {
        fn sl_shape_spans(argb: *const u8, length: usize, width: i32, rows: i32, threshold: u8, spans: *mut Span) -> usize;
    }

    fn shape_spans(argb: &[u8], width: i32, rows: i32, threshold: u8) -> Vec<Span> {
        if width <= 0 || rows <= 0 {
            return Vec::new();
        }
        let mut spans = vec![Span::default(); rows as usize];
        let count = unsafe { sl_shape_spans(argb.as_ptr(), argb.len(), width, rows, threshold, spans.as_mut_ptr()) };
        spans.truncate(count);
        spans
    }

    fn reference(argb: &[u8], background: u32) -> Vec<u8> {
        let [red, green, blue] = [
            ((background >> 16) & 0xff) as f32,
            ((background >> 8) & 0xff) as f32,
            (background & 0xff) as f32,
        ];
        let mut data = Vec::new();
        for pixel in argb.as_chunks::<4>().0 {
            let covered = 1.0 - pixel[3] as f32 / 255.0;
            data.push((pixel[0] as f32 + blue * covered).round().min(255.0) as u8);
            data.push((pixel[1] as f32 + green * covered).round().min(255.0) as u8);
            data.push((pixel[2] as f32 + red * covered).round().min(255.0) as u8);
            data.push(0);
        }
        data
    }

    #[test]
    fn compositing_matches_the_float_formula_for_every_alpha_and_channel_value() {
        let mut argb = Vec::new();
        for value in (0..=255u16).step_by(5) {
            for alpha in 0..=255u16 {
                argb.extend_from_slice(&[value as u8, (255 - value) as u8, (value / 2) as u8, alpha as u8]);
            }
        }
        for background in [0x00_00_00, 0xff_ff_ff, 0x6750a4, 0x123456] {
            assert_eq!(composite_over(&argb, background), reference(&argb, background));
        }
    }

    #[test]
    fn a_covered_pixel_keeps_its_color_and_a_clear_one_becomes_the_background() {
        let out = composite_over(&[10, 20, 30, 255, 0, 0, 0, 0], 0x112233);
        assert_eq!(out, vec![10, 20, 30, 0, 0x33, 0x22, 0x11, 0]);
    }

    #[test]
    fn premultiplied_values_above_the_background_are_clamped() {
        let out = composite_over(&[250, 250, 250, 10], 0xffffff);
        assert_eq!(&out[..3], &[255, 255, 255]);
    }

    fn rows(alphas: &[&[u8]]) -> Vec<u8> {
        alphas.iter().flat_map(|row| row.iter().flat_map(|alpha| [0, 0, 0, *alpha])).collect()
    }

    #[test]
    fn spans_cover_the_first_to_the_last_opaque_column_of_each_row() {
        let argb = rows(&[&[0, 0, 200, 255, 255, 200, 0, 0], &[0, 255, 255, 255, 255, 255, 255, 0], &[255; 8]]);
        let spans = shape_spans(&argb, 8, 3, 128);
        assert_eq!(
            spans,
            vec![
                Span { row: 0, first: 2, last: 5 },
                Span { row: 1, first: 1, last: 6 },
                Span { row: 2, first: 0, last: 7 },
            ]
        );
    }

    #[test]
    fn fully_transparent_rows_are_skipped_and_the_threshold_is_inclusive() {
        let argb = rows(&[&[0, 0, 0, 0], &[127, 128, 0, 0]]);
        assert_eq!(shape_spans(&argb, 4, 2, 128), vec![Span { row: 1, first: 1, last: 1 }]);
    }

    #[test]
    fn a_buffer_that_is_too_short_or_a_bad_size_gives_no_spans_instead_of_reading_past_it() {
        let argb = rows(&[&[255, 255]]);
        assert!(shape_spans(&argb, 2, 5, 128).is_empty());
        assert!(shape_spans(&argb, 0, 1, 128).is_empty());
        assert!(shape_spans(&argb, 2, -1, 128).is_empty());
    }
}
