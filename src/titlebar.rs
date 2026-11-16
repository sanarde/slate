// SPDX-License-Identifier: GPL-3.0-only

use smithay::utils::{Logical, Rectangle};

use crate::font::Fonts;
use crate::config::{Align, ButtonKind, ButtonShape, Color, Decoration};

pub struct Params<'a> {
    pub width: usize,
    pub height: usize,
    pub frame_left: f32,
    pub frame_top: f32,
    pub frame_right: f32,
    pub radius: f32,
    pub border_color: Color,
    pub scale: f32,
    pub decoration: &'a Decoration,
    pub fonts: &'a Fonts,
    pub focused: bool,
    pub maximized: bool,
    pub title: &'a str,
    pub hover: Option<ButtonKind>,
    pub pressed: bool,
}

pub fn layout(decoration: &Decoration, width: i32, height: i32) -> Vec<(ButtonKind, Rectangle<i32, Logical>)> {
    let size = decoration.button_size as i32;
    let spacing = decoration.button_spacing as i32;
    let margin = decoration.button_margin as i32;
    let y = (height - size) / 2;
    let mut buttons = Vec::new();

    let mut x = margin;
    for kind in &decoration.buttons_left {
        buttons.push((*kind, Rectangle::new((x, y).into(), (size, size).into())));
        x += size + spacing;
    }

    let count = decoration.buttons_right.len() as i32;
    let total = count * size + (count - 1).max(0) * spacing;
    let mut x = width - margin - total;
    for kind in &decoration.buttons_right {
        buttons.push((*kind, Rectangle::new((x, y).into(), (size, size).into())));
        x += size + spacing;
    }
    buttons
}

pub fn title_span(decoration: &Decoration, buttons: &[(ButtonKind, Rectangle<i32, Logical>)], width: i32) -> (i32, i32) {
    let padding = decoration.title_padding as i32;
    let middle = width / 2;
    let left = buttons
        .iter()
        .filter(|(_, rect)| rect.loc.x + rect.size.w / 2 < middle)
        .map(|(_, rect)| rect.loc.x + rect.size.w)
        .max()
        .unwrap_or(0);
    let right = buttons
        .iter()
        .filter(|(_, rect)| rect.loc.x + rect.size.w / 2 >= middle)
        .map(|(_, rect)| rect.loc.x)
        .min()
        .unwrap_or(width);
    (left + padding, right - padding)
}

struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<f32>,
}

impl Canvas {
    fn blend(&mut self, x: usize, y: usize, color: Color, coverage: f32) {
        if coverage <= 0.0 || x >= self.width || y >= self.height {
            return;
        }
        let alpha = color.a as f32 / 255.0 * coverage.min(1.0);
        let source = [
            color.r as f32 / 255.0 * alpha,
            color.g as f32 / 255.0 * alpha,
            color.b as f32 / 255.0 * alpha,
            alpha,
        ];
        let offset = (y * self.width + x) * 4;
        for (channel, value) in source.iter().enumerate() {
            self.pixels[offset + channel] = value + self.pixels[offset + channel] * (1.0 - alpha);
        }
    }

    fn fill(&mut self, color: Color, distance: impl Fn(f32, f32) -> f32) {
        for y in 0..self.height {
            for x in 0..self.width {
                let d = distance(x as f32 + 0.5, y as f32 + 0.5);
                self.blend(x, y, color, coverage(d));
            }
        }
    }

    fn into_argb(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len());
        for pixel in self.pixels.as_chunks::<4>().0 {
            for channel in [pixel[2], pixel[1], pixel[0], pixel[3]] {
                bytes.push((channel.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        bytes
    }
}

fn coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

fn rounded_rect(x: f32, y: f32, cx: f32, cy: f32, half_width: f32, half_height: f32, radius: f32) -> f32 {
    let qx = (x - cx).abs() - half_width + radius;
    let qy = (y - cy).abs() - half_height + radius;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

fn segment(x: f32, y: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let length = (dx * dx + dy * dy).max(f32::EPSILON);
    let t = (((x - ax) * dx + (y - ay) * dy) / length).clamp(0.0, 1.0);
    (x - (ax + t * dx)).hypot(y - (ay + t * dy))
}

struct Icon {
    kind: ButtonKind,
    maximized: bool,
    cx: f32,
    cy: f32,
    half: f32,
    stroke: f32,
    color: Color,
}

fn draw_icon(canvas: &mut Canvas, icon: &Icon) {
    let Icon {
        kind,
        maximized,
        cx,
        cy,
        half,
        stroke,
        color,
    } = *icon;
    let k = half * 0.38;
    let line = |canvas: &mut Canvas, ax: f32, ay: f32, bx: f32, by: f32| {
        canvas.fill(color, |x, y| segment(x, y, ax, ay, bx, by) - stroke / 2.0);
    };

    match kind {
        ButtonKind::Close => {
            line(canvas, cx - k, cy - k, cx + k, cy + k);
            line(canvas, cx - k, cy + k, cx + k, cy - k);
        }
        ButtonKind::Minimize => {
            line(canvas, cx - k, cy + k * 0.6, cx + k, cy + k * 0.6);
        }
        ButtonKind::Maximize if !maximized => {
            canvas.fill(color, |x, y| rounded_rect(x, y, cx, cy, k, k, 0.0).abs() - stroke / 2.0);
        }
        ButtonKind::Maximize => {
            let s = k * 0.78;
            let offset = k * 0.28;
            let (bx, by) = (cx + offset, cy - offset);
            line(canvas, bx - s, by - s, bx + s, by - s);
            line(canvas, bx + s, by - s, bx + s, by + s);
            let (fx, fy) = (cx - offset, cy + offset);
            canvas.fill(color, |x, y| rounded_rect(x, y, fx, fy, s, s, 0.0).abs() - stroke / 2.0);
        }
    }
}

pub fn render(params: &Params) -> Vec<u8> {
    let decoration = params.decoration;
    let scale = params.scale;
    let width = params.width as f32;
    let height = params.height as f32;
    let mut canvas = Canvas {
        width: params.width,
        height: params.height,
        pixels: vec![0.0; params.width * params.height * 4],
    };

    let interior_width = width - params.frame_left - params.frame_right;
    let interior_height = height - params.frame_top;
    let outer_radius = params.radius.min(width / 2.0);
    let inner_radius = (outer_radius - params.frame_top).max(0.0);

    if params.frame_left > 0.0 || params.frame_top > 0.0 || params.frame_right > 0.0 {
        let extended = height + outer_radius;
        canvas.fill(params.border_color, |x, y| {
            rounded_rect(x, y, width / 2.0, extended / 2.0, width / 2.0, extended / 2.0, outer_radius)
        });
    }

    let background = if params.focused {
        decoration.titlebar_color_focused
    } else {
        decoration.titlebar_color_unfocused
    };
    let inner_extended = interior_height + inner_radius;
    canvas.fill(background, |x, y| {
        rounded_rect(
            x,
            y,
            params.frame_left + interior_width / 2.0,
            params.frame_top + inner_extended / 2.0,
            interior_width / 2.0,
            inner_extended / 2.0,
            inner_radius,
        )
    });

    let logical_width = (interior_width / scale).round() as i32;
    let logical_height = (interior_height / scale).round() as i32;
    let buttons = layout(decoration, logical_width, logical_height);

    let icon_color = if params.focused {
        decoration.button_icon_color_focused
    } else {
        decoration.button_icon_color_unfocused
    };
    let stroke = (1.4 * scale).max(1.0);
    for (kind, rect) in &buttons {
        let cx = params.frame_left + (rect.loc.x as f32 + rect.size.w as f32 / 2.0) * scale;
        let cy = params.frame_top + (rect.loc.y as f32 + rect.size.h as f32 / 2.0) * scale;
        let half = rect.size.w as f32 / 2.0 * scale;

        let fill = if params.hover == Some(*kind) {
            match kind {
                ButtonKind::Close => decoration.button_close_hover_color,
                ButtonKind::Maximize | ButtonKind::Minimize if params.pressed => decoration.button_pressed_color,
                ButtonKind::Maximize | ButtonKind::Minimize => decoration.button_hover_color,
            }
        } else {
            decoration.button_color
        };
        let corner = match decoration.button_shape {
            ButtonShape::Circle => half,
            ButtonShape::Rounded => half * 0.35,
            ButtonShape::Square => 0.0,
        };
        canvas.fill(fill, |x, y| rounded_rect(x, y, cx, cy, half, half, corner));
        draw_icon(
            &mut canvas,
            &Icon {
                kind: *kind,
                maximized: params.maximized,
                cx,
                cy,
                half,
                stroke,
                color: icon_color,
            },
        );
    }

    draw_title(&mut canvas, params, &buttons, logical_width, interior_width, interior_height);
    canvas.into_argb()
}

fn draw_title(
    canvas: &mut Canvas,
    params: &Params,
    buttons: &[(ButtonKind, Rectangle<i32, Logical>)],
    logical_width: i32,
    interior_width: f32,
    interior_height: f32,
) {
    let decoration = params.decoration;
    let size = decoration.title_font_size as f32 * params.scale;
    let Some((ascent, descent)) = params.fonts.ascent_and_descent(size) else {
        return;
    };

    let (from, to) = title_span(decoration, buttons, logical_width);
    let from = params.frame_left + from as f32 * params.scale;
    let to = params.frame_left + to as f32 * params.scale;
    if to <= from {
        return;
    }

    let layout = params.fonts.layout(params.title, size, to - from);
    let start = match decoration.title_align {
        Align::Left => from,
        Align::Right => to - layout.width,
        Align::Center => {
            (params.frame_left + interior_width / 2.0 - layout.width / 2.0).clamp(from, (to - layout.width).max(from))
        }
    };
    let baseline = params.frame_top + (interior_height - (ascent - descent)) / 2.0 + ascent;

    let color = if params.focused {
        decoration.title_color_focused
    } else {
        decoration.title_color_unfocused
    };
    for glyph in &layout.glyphs {
        let (metrics, bitmap) = params.fonts.rasterize(glyph, size);
        let left = (start + glyph.x).round() as i32 + metrics.xmin;
        let top = (baseline - metrics.height as f32 - metrics.ymin as f32).round() as i32;
        for row in 0..metrics.height {
            for column in 0..metrics.width {
                let x = left + column as i32;
                let y = top + row as i32;
                if x < 0 || y < 0 {
                    continue;
                }
                let value = bitmap[row * metrics.width + column] as f32 / 255.0;
                canvas.blend(x as usize, y as usize, color, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params<'a>(decoration: &'a Decoration, fonts: &'a Fonts) -> Params<'a> {
        Params {
            width: 200,
            height: 30,
            frame_left: 0.0,
            frame_top: 0.0,
            frame_right: 0.0,
            radius: 8.0,
            border_color: Color::rgb(0),
            scale: 1.0,
            decoration,
            fonts,
            focused: true,
            maximized: false,
            title: "",
            hover: None,
            pressed: false,
        }
    }

    #[test]
    fn right_buttons_end_at_the_margin() {
        let decoration = Decoration::default();
        let buttons = layout(&decoration, 200, 30);

        let (kind, close) = buttons.last().unwrap();
        assert!(*kind == ButtonKind::Close);
        assert_eq!(close.loc.x + close.size.w, 200 - decoration.button_margin as i32);

        let (kind, maximize) = &buttons[1];
        assert!(*kind == ButtonKind::Maximize);
        assert_eq!(
            close.loc.x - (maximize.loc.x + maximize.size.w),
            decoration.button_spacing as i32
        );
    }

    #[test]
    fn title_stays_between_the_buttons() {
        let decoration = Decoration::default();
        let buttons = layout(&decoration, 200, 30);
        let (from, to) = title_span(&decoration, &buttons, 200);

        assert_eq!(from, decoration.title_padding as i32);
        assert_eq!(to, buttons[0].1.loc.x - decoration.title_padding as i32);
        assert!(buttons[0].0 == ButtonKind::Minimize);
    }

    #[test]
    fn upper_corners_are_transparent_and_the_body_is_filled() {
        let decoration = Decoration::default();
        let fonts = Fonts::load("");
        let pixels = render(&params(&decoration, &fonts));

        assert_eq!(pixels.len(), 200 * 30 * 4);
        assert_eq!(pixels[3], 0);

        let center = (15 * 200 + 40) * 4;
        let color = decoration.titlebar_color_focused;
        assert_eq!(&pixels[center..center + 4], &[color.b, color.g, color.r, 255]);
    }
}
