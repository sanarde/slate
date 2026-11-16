// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

use x11rb::NONE;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::{Drag, Edges, Result, Wm};
use crate::config::{ButtonKind, ClickAction, Color, DecorationMode, RuleMode};
use crate::titlebar::{self, Params};
use crate::x11::framing;
use crate::x11::geometry::Rect;

const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const MOTIF_DECORATIONS: u32 = 2;
const BUTTON_LEFT: u8 = 1;
const BUTTON_MIDDLE: u8 = 2;
const BUTTON_RIGHT: u8 = 3;
const SHAPE_CORNER_ALPHA: u8 = 128;

pub(super) use crate::x11::framing::Insets;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum FrameHit {
    Button(ButtonKind),
    Titlebar,
    Resize(Edges),
}

#[derive(Clone, PartialEq)]
pub(super) struct Painted {
    width: i32,
    height: i32,
    focused: bool,
    maximized: bool,
    title: String,
    hover: Option<ButtonKind>,
    pressed: bool,
    generation: u64,
}

fn pixel_of(color: u32) -> u32 {
    color & 0x00ff_ffff
}

impl Wm {
    pub(super) fn class_of(&self, window: Window) -> String {
        self.property_strings(window, AtomEnum::WM_CLASS.into())
            .get(1)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn should_frame(&self, window: Window) -> bool {
        let decoration = &self.settings.decoration;
        if decoration.mode == DecorationMode::Client {
            return false;
        }
        let class = self.class_of(window);
        if crate::rules::apply(&self.settings.window_rules, &class, &self.read_title(window)).no_decoration {
            return false;
        }
        if matches!(
            decoration.rules.iter().find(|(candidate, _)| *candidate == class),
            Some((_, RuleMode::Client | RuleMode::None))
        ) {
            return false;
        }

        let motif: Vec<u32> = self
            .conn
            .get_property(false, window, self.atoms._MOTIF_WM_HINTS, self.atoms._MOTIF_WM_HINTS, 0, 5)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().map(|values| values.collect()))
            .unwrap_or_default();
        !(motif.len() >= 3 && motif[0] & MOTIF_DECORATIONS != 0 && motif[2] == 0)
    }

    pub(super) fn top_of(&self, window: Window) -> Window {
        self.clients
            .iter()
            .find(|client| client.window == window)
            .and_then(|client| client.frame)
            .unwrap_or(window)
    }

    pub(super) fn client_of_frame(&self, frame: Window) -> Option<Window> {
        self.clients
            .iter()
            .find(|client| client.frame == Some(frame))
            .map(|client| client.window)
    }

    pub(super) fn insets_when_framed(&self, maximized: bool, fullscreen: bool) -> Insets {
        framing::insets(
            maximized,
            fullscreen,
            self.settings.border_width as i32,
            self.settings.decoration.titlebar_height as i32,
        )
    }

    pub(super) fn insets_for(&self, index: usize) -> Insets {
        let client = &self.clients[index];
        if client.frame.is_none() {
            return Insets::default();
        }
        self.insets_when_framed(client.maximized, client.fullscreen)
    }

    pub(super) fn map_client(&mut self, window: Window) -> Result<()> {
        match self.clients.iter().find(|client| client.window == window).and_then(|client| client.frame) {
            Some(frame) => {
                self.conn.map_window(window)?;
                self.conn.map_window(frame)?;
            }
            None => {
                self.conn.map_window(window)?;
            }
        }
        Ok(())
    }

    pub(super) fn unmap_client(&mut self, window: Window) -> Result<()> {
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        match client.frame {
            Some(frame) => {
                self.conn.unmap_window(frame)?;
            }
            None => {
                client.pending_unmaps += 1;
                self.conn.unmap_window(window)?;
            }
        }
        Ok(())
    }

    pub(super) fn set_geometry(&mut self, window: Window, rect: Rect, border: u32) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        self.clients[index].rect = rect;

        let Some(frame) = self.clients[index].frame else {
            self.conn.configure_window(
                window,
                &ConfigureWindowAux::new()
                    .x(rect.x)
                    .y(rect.y)
                    .width(rect.w)
                    .height(rect.h)
                    .border_width(border),
            )?;
            return Ok(());
        };

        let insets = self.insets_for(index);
        self.conn.configure_window(
            frame,
            &ConfigureWindowAux::new()
                .x(rect.x - insets.left)
                .y(rect.y - insets.top)
                .width((rect.w as i32 + insets.left + insets.right) as u32)
                .height((rect.h as i32 + insets.top + insets.bottom) as u32),
        )?;
        self.conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(insets.left)
                .y(insets.top)
                .width(rect.w)
                .height(rect.h)
                .border_width(0),
        )?;
        self.conn.change_property32(
            PropMode::REPLACE,
            window,
            self.atoms._NET_FRAME_EXTENTS,
            AtomEnum::CARDINAL,
            &[insets.left as u32, insets.right as u32, insets.top as u32, insets.bottom as u32],
        )?;
        self.send_configure_notify(window)?;
        self.repaint(window)
    }

    pub(super) fn read_title(&self, window: Window) -> String {
        let read = |property: Atom, kind: Atom| -> String {
            self.conn
                .get_property(false, window, property, kind, 0, 1024)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| String::from_utf8_lossy(&reply.value).into_owned())
                .unwrap_or_default()
        };

        let title = read(self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING);
        if title.is_empty() {
            return read(AtomEnum::WM_NAME.into(), AtomEnum::STRING.into());
        }
        title
    }

    pub(super) fn update_title(&mut self, window: Window) -> Result<()> {
        let title = self.read_title(window);
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        if client.title == title {
            return Ok(());
        }
        client.title = title;
        self.repaint(window)
    }

    pub(super) fn frame_client(&mut self, window: Window, mapped: bool) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        if self.clients[index].frame.is_some() || !self.should_frame(window) {
            return Ok(());
        }

        let (maximized, fullscreen, rect, hidden) = {
            let client = &self.clients[index];
            (client.maximized, client.fullscreen, client.rect, client.hidden)
        };
        let insets = self.insets_when_framed(maximized, fullscreen);
        let frame = self.conn.generate_id()?;
        let border = pixel_of(self.settings.border_focused);

        self.conn.create_window(
            self.depth,
            frame,
            self.root,
            (rect.x - insets.left) as i16,
            (rect.y - insets.top) as i16,
            (rect.w as i32 + insets.left + insets.right) as u16,
            (rect.h as i32 + insets.top + insets.bottom) as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(border)
                .override_redirect(1)
                .event_mask(
                    EventMask::SUBSTRUCTURE_REDIRECT
                        | EventMask::SUBSTRUCTURE_NOTIFY
                        | EventMask::EXPOSURE
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION
                        | EventMask::LEAVE_WINDOW,
                ),
        )?;

        if mapped {
            self.clients[index].pending_unmaps += 1;
        }
        self.conn.change_save_set(SetMode::INSERT, window)?;
        self.conn.reparent_window(window, frame, insets.left as i16, insets.top as i16)?;
        self.conn.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;

        self.clients[index].frame = Some(frame);
        self.clients[index].title = self.read_title(window);
        self.clients[index].painted = None;
        if hidden {
            self.conn.map_window(window)?;
        }
        self.set_geometry(window, rect, 0)
    }

    pub(super) fn unframe_client(&mut self, window: Window) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let Some(frame) = self.clients[index].frame.take() else {
            return Ok(());
        };

        let rect = self.clients[index].rect;
        if self.clients[index].hidden {
            self.clients[index].pending_unmaps += 1;
            self.conn.unmap_window(window)?;
        } else {
            self.clients[index].pending_unmaps += 1;
        }
        self.clients[index].painted = None;
        self.conn.reparent_window(window, self.root, rect.x as i16, rect.y as i16)?;
        self.conn.change_save_set(SetMode::DELETE, window)?;
        self.conn.destroy_window(frame)?;
        self.conn.delete_property(window, self.atoms._NET_FRAME_EXTENTS)?;

        if self.hover.as_ref().is_some_and(|(hovered, _)| *hovered == window) {
            self.hover = None;
        }
        self.set_geometry(window, rect, self.settings.border_width)?;
        if !self.clients[index].hidden {
            self.conn.map_window(window)?;
        }
        Ok(())
    }

    pub(super) fn sync_frame(&mut self, window: Window) -> Result<()> {
        let Some(client) = self.clients.iter().find(|client| client.window == window) else {
            return Ok(());
        };
        let framed = client.frame.is_some();
        let hidden = client.hidden;
        let should = self.should_frame(window);

        if should && !framed {
            self.frame_client(window, !hidden)?;
            if !hidden {
                self.map_client(window)?;
            }
        } else if !should && framed {
            self.unframe_client(window)?;
        }
        Ok(())
    }

    fn color_of(&self, color: u32) -> Color {
        Color::rgb(color)
    }

    pub(super) fn repaint(&mut self, window: Window) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let Some(frame) = self.clients[index].frame else {
            return Ok(());
        };

        let insets = self.insets_for(index);
        let rect = self.clients[index].rect;
        let width = rect.w as i32 + insets.left + insets.right;
        let height = rect.h as i32 + insets.top + insets.bottom;
        let titlebar = self.settings.decoration.titlebar_height as i32;
        let focused = self.focused == Some(window);
        let maximized = self.clients[index].maximized;
        let hover = match self.hover {
            Some((hovered, FrameHit::Button(kind))) if hovered == window => Some(kind),
            _ => None,
        };

        let key = Painted {
            width,
            height,
            focused,
            maximized,
            title: self.clients[index].title.clone(),
            hover,
            pressed: hover.is_some() && matches!(self.pressed, Some((pressed, kind)) if pressed == window && Some(kind) == hover),
            generation: self.paint_generation,
        };
        if self.clients[index].painted.as_ref() == Some(&key) {
            return Ok(());
        }

        let border = if focused {
            self.settings.border_focused
        } else {
            self.settings.border_unfocused
        };
        self.conn
            .change_window_attributes(frame, &ChangeWindowAttributesAux::new().background_pixel(pixel_of(border)))?;

        let top = insets.top;
        let strips = [
            (0, top, insets.left, height - top),
            (width - insets.right, top, insets.right, height - top),
            (0, height - insets.bottom, width, insets.bottom),
        ];
        for (x, y, w, h) in strips {
            if w > 0 && h > 0 {
                self.conn.clear_area(false, frame, x as i16, y as i16, w as u16, h as u16)?;
            }
        }

        let top_border = if top > 0 { top - titlebar } else { 0 };
        if titlebar > 0 && top > 0 {
            let radius = if maximized {
                0.0
            } else {
                self.settings.decoration.corner_radius as f32
            };
            let pixels = titlebar::render(&Params {
                width: width as usize,
                height: top as usize,
                frame_left: insets.left as f32,
                frame_top: top_border as f32,
                frame_right: insets.right as f32,
                radius,
                border_color: self.color_of(border),
                scale: 1.0,
                decoration: &self.settings.decoration,
                fonts: &self.fonts,
                focused,
                maximized,
                title: &key.title,
                hover,
                pressed: key.pressed,
            });
            self.update_shape(index, frame, [width, height, top, radius as i32], &pixels)?;
            self.put_titlebar(frame, width, top, &pixels, border)?;
        } else if top > 0 {
            self.conn.clear_area(false, frame, 0, 0, width as u16, top as u16)?;
        }

        self.clients[index].painted = Some(key);
        Ok(())
    }

    fn put_titlebar(&self, frame: Window, width: i32, height: i32, argb: &[u8], background: u32) -> Result<()> {
        let [bg_red, bg_green, bg_blue] = [
            ((background >> 16) & 0xff) as f32,
            ((background >> 8) & 0xff) as f32,
            (background & 0xff) as f32,
        ];

        let mut data = Vec::with_capacity(argb.len());
        for pixel in argb.as_chunks::<4>().0 {
            let covered = 1.0 - pixel[3] as f32 / 255.0;
            data.push((pixel[0] as f32 + bg_blue * covered).round().min(255.0) as u8);
            data.push((pixel[1] as f32 + bg_green * covered).round().min(255.0) as u8);
            data.push((pixel[2] as f32 + bg_red * covered).round().min(255.0) as u8);
            data.push(0);
        }

        let row_bytes = width as usize * 4;
        let budget = (self.conn.maximum_request_bytes().saturating_sub(64)).max(row_bytes);
        let rows_per_request = (budget / row_bytes).clamp(1, height as usize);
        let mut row = 0;
        while row < height as usize {
            let rows = rows_per_request.min(height as usize - row);
            self.conn.put_image(
                ImageFormat::Z_PIXMAP,
                frame,
                self.gc,
                width as u16,
                rows as u16,
                0,
                row as i16,
                0,
                self.depth,
                &data[row * row_bytes..(row + rows) * row_bytes],
            )?;
            row += rows;
        }
        Ok(())
    }

    fn update_shape(&mut self, index: usize, frame: Window, size: [i32; 4], argb: &[u8]) -> Result<()> {
        let [width, height, top, radius] = size;
        if !self.shape_available {
            return Ok(());
        }
        let wanted = (radius > 0).then_some((width, height, radius));
        if self.clients[index].shaped == wanted {
            return Ok(());
        }
        self.clients[index].shaped = wanted;

        let Some((_, _, radius)) = wanted else {
            self.conn
                .shape_mask(shape::SO::SET, shape::SK::BOUNDING, frame, 0, 0, NONE)?;
            return Ok(());
        };

        let corner_rows = (radius + 1).min(top);
        let mut rectangles = Vec::new();
        for row in 0..corner_rows {
            let alpha = |column: i32| argb[((row * width + column) * 4 + 3) as usize];
            let first = (0..width).find(|column| alpha(*column) >= SHAPE_CORNER_ALPHA);
            let last = (0..width).rev().find(|column| alpha(*column) >= SHAPE_CORNER_ALPHA);
            if let (Some(first), Some(last)) = (first, last) {
                rectangles.push(Rectangle {
                    x: first as i16,
                    y: row as i16,
                    width: (last - first + 1) as u16,
                    height: 1,
                });
            }
        }
        rectangles.push(Rectangle {
            x: 0,
            y: corner_rows as i16,
            width: width as u16,
            height: (height - corner_rows) as u16,
        });
        self.conn.shape_rectangles(
            shape::SO::SET,
            shape::SK::BOUNDING,
            ClipOrdering::UNSORTED,
            frame,
            0,
            0,
            &rectangles,
        )?;
        Ok(())
    }

    pub(super) fn frame_hit(&self, window: Window, x: i32, y: i32) -> Option<FrameHit> {
        let index = self.clients.iter().position(|client| client.window == window)?;
        let insets = self.insets_for(index);
        let client = &self.clients[index];
        if client.frame.is_none() || client.fullscreen {
            return None;
        }

        let rect = client.rect;
        let width = rect.w as i32 + insets.left + insets.right;
        let height = rect.h as i32 + insets.top + insets.bottom;
        let titlebar = self.settings.decoration.titlebar_height as i32;
        let top_border = if insets.top > 0 { insets.top - titlebar } else { 0 };

        if !client.maximized {
            let thickness = (self.settings.border_width as i32).max(1);
            let top_zone = thickness + self.settings.decoration.resize_margin as i32;
            let bits = framing::edges(width, height, x, y, thickness, top_zone);
            if bits != 0 {
                return Some(FrameHit::Resize(Edges::from_bits(bits)));
            }
        }

        if titlebar == 0 {
            return None;
        }
        let relative_x = x - insets.left;
        let relative_y = y - top_border;
        for (kind, button) in titlebar::layout(&self.settings.decoration, rect.w as i32, titlebar) {
            let inside = relative_x >= button.loc.x
                && relative_x < button.loc.x + button.size.w
                && relative_y >= button.loc.y
                && relative_y < button.loc.y + button.size.h;
            if inside {
                return Some(FrameHit::Button(kind));
            }
        }
        Some(FrameHit::Titlebar)
    }

    fn cursor_for(&self, hit: Option<FrameHit>) -> Cursor {
        let glyph = match hit {
            Some(FrameHit::Resize(edges)) => framing::cursor_glyph(edges.bits()) as u16,
            _ => 68,
        };
        self.cursors.get(&glyph).copied().unwrap_or(NONE)
    }

    pub(super) fn frame_motion(&mut self, event: &MotionNotifyEvent) -> Result<()> {
        let Some(window) = self.client_of_frame(event.event) else {
            return Ok(());
        };
        let hit = self.frame_hit(window, event.event_x as i32, event.event_y as i32);
        let new = hit.map(|hit| (window, hit));
        if new == self.hover {
            return Ok(());
        }

        let previous = self.hover.take().map(|(previous, _)| previous);
        self.hover = new;
        let cursor = self.cursor_for(hit);
        self.conn
            .change_window_attributes(event.event, &ChangeWindowAttributesAux::new().cursor(cursor))?;
        if let Some(previous) = previous.filter(|previous| *previous != window) {
            self.repaint(previous)?;
        }
        self.repaint(window)
    }

    pub(super) fn frame_leave(&mut self, frame: Window) -> Result<()> {
        let Some(window) = self.client_of_frame(frame) else {
            return Ok(());
        };
        if self.drag.is_some() || !self.hover.as_ref().is_some_and(|(hovered, _)| *hovered == window) {
            return Ok(());
        }
        self.hover = None;
        self.repaint(window)
    }

    fn perform(&mut self, window: Window, action: ClickAction) -> Result<()> {
        let Some(client) = self.clients.iter().find(|client| client.window == window) else {
            return Ok(());
        };
        let (maximized, fullscreen) = (client.maximized, client.fullscreen);
        match action {
            ClickAction::None => Ok(()),
            ClickAction::Maximize => self.set_state(window, Some(!maximized), None),
            ClickAction::Fullscreen => self.set_state(window, None, Some(!fullscreen)),
            ClickAction::Minimize => self.minimize(window),
            ClickAction::Close => self.close(window),
        }
    }

    pub(super) fn frame_press(&mut self, event: &ButtonPressEvent) -> Result<()> {
        let Some(window) = self.client_of_frame(event.event) else {
            return Ok(());
        };
        let hit = self.frame_hit(window, event.event_x as i32, event.event_y as i32);
        self.focus(window)?;

        let pointer = (event.root_x as i32, event.root_y as i32);
        match hit {
            Some(FrameHit::Button(kind)) if event.detail == BUTTON_LEFT => {
                self.pressed = Some((window, kind));
                self.repaint(window)?;
            }
            Some(FrameHit::Titlebar) => match event.detail {
                BUTTON_LEFT => {
                    let double = self
                        .last_title_click
                        .take()
                        .is_some_and(|(previous, at)| previous == window && at.elapsed() < DOUBLE_CLICK);
                    if double {
                        let action = self.settings.decoration.double_click;
                        return self.perform(window, action);
                    }
                    self.last_title_click = Some((window, Instant::now()));

                    let Some(client) = self.clients.iter().find(|client| client.window == window) else {
                        return Ok(());
                    };
                    if client.maximized || client.fullscreen {
                        return Ok(());
                    }
                    if client.tiled {
                        self.make_floating(window)?;
                    }
                    let initial = self
                        .clients
                        .iter()
                        .find(|client| client.window == window)
                        .map(|client| client.rect)
                        .unwrap_or_default();
                    self.drag = Some(Drag {
                        window,
                        resize: None,
                        pointer,
                        initial,
                    });
                }
                BUTTON_MIDDLE => {
                    let action = self.settings.decoration.middle_click;
                    return self.perform(window, action);
                }
                BUTTON_RIGHT => {
                    let action = self.settings.decoration.right_click;
                    return self.perform(window, action);
                }
                _ => {}
            },
            Some(FrameHit::Resize(edges)) if event.detail == BUTTON_LEFT => {
                let Some(client) = self.clients.iter().find(|client| client.window == window) else {
                    return Ok(());
                };
                if client.tiled || client.maximized || client.fullscreen {
                    return Ok(());
                }
                self.drag = Some(Drag {
                    window,
                    resize: Some(edges),
                    pointer,
                    initial: client.rect,
                });
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn frame_release(&mut self) -> Result<()> {
        let Some((window, kind)) = self.pressed.take() else {
            return Ok(());
        };
        self.repaint(window)?;
        if self.hover != Some((window, FrameHit::Button(kind))) {
            return Ok(());
        }
        match kind {
            ButtonKind::Close => self.close(window),
            ButtonKind::Minimize => self.minimize(window),
            ButtonKind::Maximize => {
                let maximized = self
                    .clients
                    .iter()
                    .find(|client| client.window == window)
                    .is_some_and(|client| client.maximized);
                self.set_state(window, Some(!maximized), None)
            }
        }
    }
}
