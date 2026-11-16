// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Transform};

use super::render::OutputElement;
use super::state::Slate;
use crate::config::Color;

const MAX_CACHED_SETS: usize = 12;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    size: i32,
    radius: i32,
    bottom: i32,
    color: u32,
    scale: u64,
}

pub struct ShadowSet {
    top: MemoryRenderBuffer,
    bottom: MemoryRenderBuffer,
    left: MemoryRenderBuffer,
    right: MemoryRenderBuffer,
    corners: [MemoryRenderBuffer; 4],
}

fn falloff(distance: f32, size: f32) -> f32 {
    if distance <= 0.0 {
        return 1.0;
    }
    let t = (distance / size).min(1.0);
    (1.0 - t) * (1.0 - t)
}

fn pixel(color: Color, coverage: f32) -> [u8; 4] {
    let alpha = color.a as f32 / 255.0 * coverage;
    [
        (color.b as f32 * alpha).round() as u8,
        (color.g as f32 * alpha).round() as u8,
        (color.r as f32 * alpha).round() as u8,
        (alpha * 255.0).round() as u8,
    ]
}

fn buffer(bytes: &[u8], width: usize, height: usize) -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        bytes,
        Fourcc::Argb8888,
        (width as i32, height as i32),
        1,
        Transform::Normal,
        None,
    )
}

fn vertical_strip(size: f32, color: Color, outward_up: bool) -> MemoryRenderBuffer {
    let rows = size.ceil().max(1.0) as usize;
    let mut bytes = Vec::with_capacity(rows * 4);
    for row in 0..rows {
        let distance = if outward_up {
            rows as f32 - row as f32 - 0.5
        } else {
            row as f32 + 0.5
        };
        bytes.extend_from_slice(&pixel(color, falloff(distance, size)));
    }
    buffer(&bytes, 1, rows)
}

fn horizontal_strip(size: f32, color: Color, outward_left: bool) -> MemoryRenderBuffer {
    let columns = size.ceil().max(1.0) as usize;
    let mut bytes = Vec::with_capacity(columns * 4);
    for column in 0..columns {
        let distance = if outward_left {
            columns as f32 - column as f32 - 0.5
        } else {
            column as f32 + 0.5
        };
        bytes.extend_from_slice(&pixel(color, falloff(distance, size)));
    }
    buffer(&bytes, columns, 1)
}

fn corner(size: f32, radius: f32, color: Color, flip_x: bool, flip_y: bool) -> MemoryRenderBuffer {
    let side = (size + radius).ceil().max(1.0) as usize;
    let center = size + radius;
    let mut bytes = Vec::with_capacity(side * side * 4);

    for y in 0..side {
        for x in 0..side {
            let column = if flip_x { side - 1 - x } else { x };
            let row = if flip_y { side - 1 - y } else { y };
            let (px, py) = (column as f32 + 0.5, row as f32 + 0.5);

            let qx = center - px;
            let qy = center - py;
            let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
            bytes.extend_from_slice(&pixel(color, falloff(distance, size)));
        }
    }
    buffer(&bytes, side, side)
}

fn build(key: Key, color: Color) -> ShadowSet {
    let scale = f64::from_bits(key.scale) as f32;
    let size = key.size as f32 * scale;
    let radius = key.radius as f32 * scale;
    let bottom = key.bottom as f32 * scale;

    ShadowSet {
        top: vertical_strip(size, color, true),
        bottom: vertical_strip(size, color, false),
        left: horizontal_strip(size, color, true),
        right: horizontal_strip(size, color, false),
        corners: [
            corner(size, radius, color, false, false),
            corner(size, radius, color, true, false),
            corner(size, bottom, color, false, true),
            corner(size, bottom, color, true, true),
        ],
    }
}

fn piece(
    renderer: &mut GlesRenderer,
    buffer: &MemoryRenderBuffer,
    rect: Rectangle<i32, Logical>,
    origin: Point<i32, Logical>,
    scale: f64,
    alpha: f32,
    elements: &mut Vec<OutputElement>,
) {
    if rect.size.w <= 0 || rect.size.h <= 0 {
        return;
    }
    let location = (rect.loc - origin).to_f64().to_physical(scale);
    match MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        location,
        buffer,
        Some(alpha),
        None,
        Some(rect.size),
        Kind::Unspecified,
    ) {
        Ok(element) => elements.push(OutputElement::Memory(element)),
        Err(error) => tracing::error!(%error, "failed to upload a shadow piece"),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn elements(
    state: &Slate,
    renderer: &mut GlesRenderer,
    frame: Rectangle<i32, Logical>,
    radius: i32,
    bottom: i32,
    focused: bool,
    origin: Point<i32, Logical>,
    scale: f64,
    alpha: f32,
) -> Vec<OutputElement> {
    let effects = &state.settings.effects;
    let style = if focused { &effects.shadow_focused } else { &effects.shadow_unfocused };
    let size = style.size;
    if size <= 0 {
        return Vec::new();
    }
    let color = style.color;
    let key = Key {
        size,
        radius,
        bottom,
        color: (color.r as u32) << 24 | (color.g as u32) << 16 | (color.b as u32) << 8 | color.a as u32,
        scale: scale.to_bits(),
    };

    let mut cache = state.shadows.borrow_mut();
    if cache.len() > MAX_CACHED_SETS && !cache.contains_key(&key) {
        cache.clear();
    }
    let set = cache.entry(key).or_insert_with(|| build(key, color));

    let (offset_x, offset_y) = style.offset;
    let (x, y) = (frame.loc.x + offset_x, frame.loc.y + offset_y);
    let (w, h) = (frame.size.w, frame.size.h);
    let rect = |x: i32, y: i32, w: i32, h: i32| Rectangle::new((x, y).into(), (w, h).into());

    let mut elements = Vec::new();
    piece(renderer, &set.top, rect(x + radius, y - size, w - 2 * radius, size), origin, scale, alpha, &mut elements);
    piece(renderer, &set.bottom, rect(x + bottom, y + h, w - 2 * bottom, size), origin, scale, alpha, &mut elements);
    piece(renderer, &set.left, rect(x - size, y + radius, size, h - radius - bottom), origin, scale, alpha, &mut elements);
    piece(renderer, &set.right, rect(x + w, y + radius, size, h - radius - bottom), origin, scale, alpha, &mut elements);
    piece(renderer, &set.corners[0], rect(x - size, y - size, size + radius, size + radius), origin, scale, alpha, &mut elements);
    piece(renderer, &set.corners[1], rect(x + w - radius, y - size, size + radius, size + radius), origin, scale, alpha, &mut elements);
    piece(renderer, &set.corners[2], rect(x - size, y + h - bottom, size + bottom, size + bottom), origin, scale, alpha, &mut elements);
    piece(renderer, &set.corners[3], rect(x + w - bottom, y + h - bottom, size + bottom, size + bottom), origin, scale, alpha, &mut elements);
    elements
}
