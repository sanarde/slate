// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::input::pointer::CursorIcon;
use smithay::utils::{Logical, Point, Transform};
use xcursor::CursorTheme;
use xcursor::parser::{Image, parse_xcursor};

const DEFAULT_THEME: &str = "default";
const DEFAULT_SIZE: u32 = 24;
const FALLBACK_SIZE: i32 = 8;

pub struct CursorImage {
    pub buffer: MemoryRenderBuffer,
    pub hotspot: Point<i32, Logical>,
}

pub struct Cursors {
    theme: CursorTheme,
    size: u32,
    images: HashMap<CursorIcon, CursorImage>,
}

impl Cursors {
    pub fn new() -> Cursors {
        let name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| DEFAULT_THEME.to_string());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|size| size.parse().ok())
            .unwrap_or(DEFAULT_SIZE);

        Cursors {
            theme: CursorTheme::load(&name),
            size,
            images: HashMap::new(),
        }
    }

    pub fn image(&mut self, icon: CursorIcon) -> &CursorImage {
        if !self.images.contains_key(&icon) {
            let image = self.load(icon).unwrap_or_else(fallback_image);
            self.images.insert(icon, image);
        }
        &self.images[&icon]
    }

    fn load(&self, icon: CursorIcon) -> Option<CursorImage> {
        let names = std::iter::once(icon.name())
            .chain(icon.alt_names().iter().copied())
            .chain(std::iter::once("left_ptr"));

        for name in names {
            let Some(path) = self.theme.load_icon(name) else {
                continue;
            };
            let Ok(content) = std::fs::read(&path) else {
                continue;
            };
            let Some(images) = parse_xcursor(&content) else {
                continue;
            };
            if let Some(image) = nearest_image(&images, self.size) {
                return Some(CursorImage {
                    buffer: MemoryRenderBuffer::from_slice(
                        &image.pixels_rgba,
                        Fourcc::Abgr8888,
                        (image.width as i32, image.height as i32),
                        1,
                        Transform::Normal,
                        None,
                    ),
                    hotspot: (image.xhot as i32, image.yhot as i32).into(),
                });
            }
        }
        None
    }
}

fn nearest_image(images: &[Image], size: u32) -> Option<&Image> {
    images.iter().min_by_key(|image| image.size.abs_diff(size))
}

fn fallback_image() -> CursorImage {
    let pixels = vec![0xff; (FALLBACK_SIZE * FALLBACK_SIZE * 4) as usize];
    CursorImage {
        buffer: MemoryRenderBuffer::from_slice(
            &pixels,
            Fourcc::Abgr8888,
            (FALLBACK_SIZE, FALLBACK_SIZE),
            1,
            Transform::Normal,
            None,
        ),
        hotspot: (0, 0).into(),
    }
}
