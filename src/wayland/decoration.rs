// SPDX-License-Identifier: GPL-3.0-only

use std::cell::RefCell;
use std::time::{Duration, Instant};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::Window;
use smithay::input::pointer::{CursorIcon, CursorImageStatus, GrabStartData};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as XdgMode;
use smithay::utils::{Logical, Point, Rectangle, Serial, Size, Transform};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;

use super::grabs::{self, Edges};
use super::render::OutputElement;
use super::shell;
use super::state::Slate;
use crate::titlebar::{self, Params};
use crate::config::{ButtonKind, ClickAction, Color, DecorationMode, RuleMode};

const BUTTON_LEFT: u32 = 0x110;
const BUTTON_RIGHT: u32 = 0x111;
const BUTTON_MIDDLE: u32 = 0x112;
const CORNER_SPAN: i32 = 12;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Insets {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Hit {
    Button(ButtonKind),
    Titlebar,
    Resize(Edges),
}

#[derive(PartialEq)]
struct Key {
    size: Size<i32, Logical>,
    insets: Insets,
    scale: u64,
    focused: bool,
    maximized: bool,
    title: String,
    hover: Option<ButtonKind>,
    generation: u64,
}

struct Cache {
    key: Key,
    titlebar: Option<MemoryRenderBuffer>,
    borders: Vec<(Rectangle<i32, Logical>, SolidColorBuffer)>,
    titlebar_rect: Rectangle<i32, Logical>,
    content_loc: Point<i32, Logical>,
}

pub fn app_id(window: &Window) -> String {
    if let Some(toplevel) = window.toplevel() {
        return with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().unwrap().app_id.clone())
        })
        .unwrap_or_default();
    }
    window.x11_surface().map(|surface| surface.class()).unwrap_or_default()
}

pub fn title(window: &Window) -> String {
    if let Some(toplevel) = window.toplevel() {
        return with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().unwrap().title.clone())
        })
        .unwrap_or_default();
    }
    window.x11_surface().map(|surface| surface.title()).unwrap_or_default()
}

pub fn rule_for(state: &Slate, app_id: &str) -> Option<RuleMode> {
    state
        .settings
        .decoration
        .rules
        .iter()
        .find(|(candidate, _)| candidate == app_id)
        .map(|(_, mode)| *mode)
}

pub fn negotiated_mode(state: &Slate, app_id: &str, requested: Option<XdgMode>) -> XdgMode {
    match rule_for(state, app_id) {
        Some(RuleMode::Client) => XdgMode::ClientSide,
        Some(RuleMode::Server | RuleMode::None) => XdgMode::ServerSide,
        None => match state.settings.decoration.mode {
            DecorationMode::Server => XdgMode::ServerSide,
            DecorationMode::Client => XdgMode::ClientSide,
            DecorationMode::Auto => requested.unwrap_or(XdgMode::ServerSide),
        },
    }
}

pub fn framed(state: &Slate, window: &Window) -> bool {
    if super::shell::rule_of(window).no_decoration {
        return false;
    }
    let rule = rule_for(state, &app_id(window));
    if rule == Some(RuleMode::None) {
        return false;
    }

    if let Some(toplevel) = window.toplevel() {
        return toplevel.current_state().decoration_mode == Some(XdgMode::ServerSide);
    }
    let Some(surface) = window.x11_surface() else {
        return false;
    };
    !surface.is_override_redirect()
        && surface.is_decorated()
        && rule != Some(RuleMode::Client)
        && state.settings.decoration.mode != DecorationMode::Client
}

pub fn insets_with(state: &Slate, window: &Window, maximized: bool, fullscreen: bool) -> Insets {
    if fullscreen || !framed(state, window) {
        return Insets::default();
    }
    let border = state.settings.border_width as i32;
    let titlebar = state.settings.decoration.titlebar_height as i32;
    if maximized {
        return Insets {
            top: titlebar,
            ..Insets::default()
        };
    }
    Insets {
        left: border,
        right: border,
        top: titlebar + border,
        bottom: border,
    }
}

pub fn insets(state: &Slate, window: &Window) -> Insets {
    let (maximized, fullscreen) = shell::flags(window);
    insets_with(state, window, maximized, fullscreen)
}

pub fn frame_of(content: Rectangle<i32, Logical>, insets: Insets) -> Rectangle<i32, Logical> {
    Rectangle::new(
        (content.loc.x - insets.left, content.loc.y - insets.top).into(),
        (
            content.size.w + insets.left + insets.right,
            content.size.h + insets.top + insets.bottom,
        )
            .into(),
    )
}

fn titlebar_of(state: &Slate, content: Rectangle<i32, Logical>) -> Rectangle<i32, Logical> {
    let height = state.settings.decoration.titlebar_height as i32;
    Rectangle::new((content.loc.x, content.loc.y - height).into(), (content.size.w, height).into())
}

fn border_color(state: &Slate, focused: bool) -> Color {
    Color::rgb(if focused {
        state.settings.border_focused
    } else {
        state.settings.border_unfocused
    })
}

pub fn frame_rect(state: &Slate, window: &Window) -> Option<Rectangle<i32, Logical>> {
    let content = state.space.element_geometry(window)?;
    Some(frame_of(content, insets(state, window)))
}

pub fn hit(state: &Slate, location: Point<f64, Logical>) -> Option<(Window, Hit)> {
    let point = location.to_i32_floor();
    let settings = &state.settings;

    for window in state.space.elements().rev() {
        let Some(content) = state.space.element_geometry(window) else {
            continue;
        };
        if content.contains(point) {
            return None;
        }
        if !framed(state, window) {
            continue;
        }

        let (maximized, fullscreen) = shell::flags(window);
        if fullscreen {
            continue;
        }
        let insets = insets_with(state, window, maximized, fullscreen);
        let frame = frame_of(content, insets);
        let margin = if maximized { 0 } else { settings.decoration.resize_margin as i32 };
        let outer = Rectangle::new(
            (frame.loc.x - margin, frame.loc.y - margin).into(),
            (frame.size.w + 2 * margin, frame.size.h + 2 * margin).into(),
        );
        if !outer.contains(point) {
            continue;
        }

        if !maximized {
            let thickness = (settings.border_width as i32).max(1);
            let mut edges = Edges::new(
                point.x < frame.loc.x + thickness,
                point.x >= frame.loc.x + frame.size.w - thickness,
                point.y < frame.loc.y + thickness,
                point.y >= frame.loc.y + frame.size.h - thickness,
            );
            if edges.any() {
                let vertical = edges.left || edges.right;
                let horizontal = edges.top || edges.bottom;
                if vertical && !horizontal {
                    edges.top = point.y < frame.loc.y + CORNER_SPAN;
                    edges.bottom = point.y >= frame.loc.y + frame.size.h - CORNER_SPAN;
                } else if horizontal && !vertical {
                    edges.left = point.x < frame.loc.x + CORNER_SPAN;
                    edges.right = point.x >= frame.loc.x + frame.size.w - CORNER_SPAN;
                }
                return Some((window.clone(), Hit::Resize(edges)));
            }
        }

        if settings.decoration.titlebar_height == 0 {
            continue;
        }
        let titlebar = titlebar_of(state, content);
        let buttons = titlebar::layout(&settings.decoration, titlebar.size.w, titlebar.size.h);
        for (kind, rect) in buttons {
            let absolute = Rectangle::new(rect.loc + titlebar.loc, rect.size);
            if absolute.contains(point) {
                return Some((window.clone(), Hit::Button(kind)));
            }
        }
        return Some((window.clone(), Hit::Titlebar));
    }
    None
}

fn cursor_for(edges: Edges) -> CursorIcon {
    match (edges.left, edges.right, edges.top, edges.bottom) {
        (true, _, true, _) => CursorIcon::NwResize,
        (_, true, true, _) => CursorIcon::NeResize,
        (true, _, _, true) => CursorIcon::SwResize,
        (_, true, _, true) => CursorIcon::SeResize,
        (true, _, _, _) => CursorIcon::WResize,
        (_, true, _, _) => CursorIcon::EResize,
        (_, _, true, _) => CursorIcon::NResize,
        _ => CursorIcon::SResize,
    }
}

pub fn update_hover(state: &mut Slate, location: Point<f64, Logical>) {
    let hit = hit(state, location);
    if hit == state.decoration_hover {
        return;
    }
    state.decoration_hover = hit.clone();

    match hit {
        Some((_, Hit::Resize(edges))) => {
            state.cursor_status = CursorImageStatus::Named(cursor_for(edges));
            state.decoration_cursor = true;
        }
        Some(_) => {
            state.cursor_status = CursorImageStatus::Named(CursorIcon::Default);
            state.decoration_cursor = true;
        }
        None if state.decoration_cursor => {
            state.cursor_status = CursorImageStatus::default_named();
            state.decoration_cursor = false;
        }
        None => {}
    }
}

fn perform(state: &mut Slate, window: &Window, action: ClickAction) {
    match action {
        ClickAction::None => {}
        ClickAction::Maximize => shell::toggle_maximized(state, window),
        ClickAction::Minimize => shell::minimize_window(state, window),
        ClickAction::Fullscreen => shell::toggle_fullscreen(state, window),
        ClickAction::Close => shell::close_window(window),
    }
}

pub fn press(state: &mut Slate, window: Window, hit: Hit, button: u32, serial: Serial, location: Point<f64, Logical>) {
    shell::focus_window(state, Some(&window), serial);
    let start_data = GrabStartData {
        focus: None,
        button,
        location,
    };
    let seat = state.seat.clone();
    let (maximized, fullscreen) = shell::flags(&window);

    match hit {
        Hit::Button(kind) if button == BUTTON_LEFT => state.decoration_press = Some((window, kind)),
        Hit::Button(_) => {}
        Hit::Titlebar => {
            let actions = &state.settings.decoration;
            match button {
                BUTTON_LEFT => {
                    let double = state
                        .last_title_click
                        .take()
                        .is_some_and(|(previous, at)| previous == window && at.elapsed() < DOUBLE_CLICK);
                    if double {
                        let action = state.settings.decoration.double_click;
                        perform(state, &window, action);
                    } else {
                        state.last_title_click = Some((window.clone(), Instant::now()));
                        if !maximized && !fullscreen {
                            grabs::start_move(state, &seat, window, serial, Some(start_data));
                        }
                    }
                }
                BUTTON_MIDDLE => {
                    let action = actions.middle_click;
                    perform(state, &window, action);
                }
                BUTTON_RIGHT => {
                    let action = actions.right_click;
                    perform(state, &window, action);
                }
                _ => {}
            }
        }
        Hit::Resize(edges) if button == BUTTON_LEFT => {
            grabs::start_resize(state, &seat, window, edges, serial, Some(start_data));
        }
        Hit::Resize(_) => {}
    }
}

pub fn release(state: &mut Slate) {
    let Some((window, kind)) = state.decoration_press.take() else {
        return;
    };
    if state.decoration_hover != Some((window.clone(), Hit::Button(kind))) {
        return;
    }
    match kind {
        ButtonKind::Close => shell::close_window(&window),
        ButtonKind::Maximize => shell::toggle_maximized(state, &window),
        ButtonKind::Minimize => shell::minimize_window(state, &window),
    }
}

fn build_cache(state: &Slate, key: Key, content: Rectangle<i32, Logical>, scale: f64, hover: Option<ButtonKind>) -> Cache {
    let decoration = &state.settings.decoration;
    let insets = key.insets;
    let titlebar_height = decoration.titlebar_height as i32;
    let top_border = if insets.top > 0 { insets.top - titlebar_height } else { 0 };

    let rect = Rectangle::new(
        (content.loc.x - insets.left, content.loc.y - titlebar_height - top_border).into(),
        (
            content.size.w + insets.left + insets.right,
            titlebar_height + top_border,
        )
            .into(),
    );
    let width = ((rect.size.w as f64 * scale).round() as usize).max(1);
    let height = ((rect.size.h as f64 * scale).round() as usize).max(1);
    let radius = if key.maximized { 0.0 } else { decoration.corner_radius as f32 * scale as f32 };

    let titlebar = (titlebar_height > 0).then(|| {
        let pixels = titlebar::render(&Params {
        width,
        height,
        frame_left: (insets.left as f64 * scale) as f32,
        frame_top: (top_border as f64 * scale) as f32,
        frame_right: (insets.right as f64 * scale) as f32,
        radius,
        border_color: border_color(state, key.focused),
        scale: scale as f32,
        decoration,
        fonts: &state.fonts,
        focused: key.focused,
        maximized: key.maximized,
        title: &key.title,
        hover,
        });
        MemoryRenderBuffer::from_slice(
            &pixels,
            Fourcc::Argb8888,
            (width as i32, height as i32),
            1,
            Transform::Normal,
            None,
        )
    });

    let mut borders = Vec::new();
    if insets.left > 0 {
        let color = border_color(state, key.focused);
        let top = if titlebar_height == 0 {
            Rectangle::new(
                (content.loc.x - insets.left, content.loc.y - insets.top).into(),
                (content.size.w + insets.left + insets.right, insets.top).into(),
            )
        } else {
            Rectangle::default()
        };
        let parts = [
            top,
            Rectangle::new(
                (content.loc.x - insets.left, content.loc.y).into(),
                (insets.left, content.size.h).into(),
            ),
            Rectangle::new(
                (content.loc.x + content.size.w, content.loc.y).into(),
                (insets.right, content.size.h).into(),
            ),
            Rectangle::new(
                (content.loc.x - insets.left, content.loc.y + content.size.h).into(),
                (content.size.w + insets.left + insets.right, insets.bottom).into(),
            ),
        ];
        for part in parts {
            if part.size.w > 0 && part.size.h > 0 {
                let buffer = SolidColorBuffer::new(
                    part.size,
                    [
                        color.r as f32 / 255.0,
                        color.g as f32 / 255.0,
                        color.b as f32 / 255.0,
                        color.a as f32 / 255.0,
                    ],
                );
                borders.push((part, buffer));
            }
        }
    }

    Cache {
        key,
        titlebar,
        borders,
        titlebar_rect: rect,
        content_loc: content.loc,
    }
}

pub fn elements(
    state: &Slate,
    renderer: &mut GlesRenderer,
    window: &Window,
    origin: Point<i32, Logical>,
    scale: f64,
    focused: bool,
    alpha: f32,
) -> Vec<OutputElement> {
    let (maximized, fullscreen) = shell::flags(window);
    if fullscreen || !framed(state, window) {
        return Vec::new();
    }
    let Some(content) = state.space.element_geometry(window) else {
        return Vec::new();
    };

    let hover = match &state.decoration_hover {
        Some((hovered, Hit::Button(kind))) if hovered == window => Some(*kind),
        _ => None,
    };
    let key = Key {
        size: content.size,
        insets: insets_with(state, window, maximized, fullscreen),
        scale: scale.to_bits(),
        focused,
        maximized,
        title: title(window),
        hover,
        generation: state.decoration_generation,
    };

    let cell = window.user_data().get_or_insert(|| RefCell::new(None::<Cache>));
    let mut cache = cell.borrow_mut();
    if !matches!(cache.as_ref(), Some(existing) if existing.key == key) {
        *cache = Some(build_cache(state, key, content, scale, hover));
    }
    let cache = cache.as_ref().unwrap();

    let mut elements = Vec::new();
    let shift = content.loc - cache.content_loc;
    if let Some(titlebar) = &cache.titlebar {
        let buffer_rect = Rectangle::new(cache.titlebar_rect.loc + shift, cache.titlebar_rect.size);
        let location = (buffer_rect.loc - origin).to_f64().to_physical(scale);
        match MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            titlebar,
            Some(alpha),
            None,
            Some(buffer_rect.size),
            Kind::Unspecified,
        ) {
            Ok(element) => elements.push(OutputElement::Memory(element)),
            Err(error) => tracing::error!(%error, "failed to upload the titlebar"),
        }
    }

    for (rect, buffer) in &cache.borders {
        let location = (rect.loc + shift - origin).to_physical_precise_round(scale);
        elements.push(OutputElement::Solid(SolidColorRenderElement::from_buffer(
            buffer,
            location,
            scale,
            alpha,
            Kind::Unspecified,
        )));
    }
    elements
}
