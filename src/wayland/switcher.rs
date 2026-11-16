// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::{Window, layer_map_for_output};
use smithay::input::keyboard::ModifiersState;
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle};

use super::animation::{Anchor, Effect};
use super::decoration;
use super::render::{self, OutputElement};
use super::shell;
use super::state::Slate;
use crate::overview::{self, Bounds};

const GAP: i32 = 24;
const ROW_CELL: (i32, i32) = (300, 190);
const BORDER: i32 = 4;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Overview,
    Cycle,
}

pub struct Switcher {
    kind: Mode,
    output: Output,
    windows: Vec<Window>,
    selected: usize,
    hold_alt: bool,
    hold_logo: bool,
}

struct Tile {
    window: Window,
    cell: Bounds,
    scale: f64,
    target: Point<i32, Logical>,
    frame: Rectangle<i32, Logical>,
}

pub fn active(state: &Slate) -> bool {
    state.switcher.is_some()
}

fn current_output(state: &Slate) -> Option<Output> {
    let location = state.seat.get_pointer()?.current_location();
    state
        .space
        .output_under(location)
        .next()
        .or_else(|| state.space.outputs().next())
        .cloned()
}

fn candidates(state: &Slate, output: &Output) -> Vec<Window> {
    state
        .space
        .elements()
        .rev()
        .filter(|window| !shell::is_override_redirect(window))
        .filter(|window| shell::output_for_window(state, window).as_ref() == Some(output))
        .cloned()
        .collect()
}

pub fn start_cycle(state: &mut Slate, forward: bool, modifiers: &ModifiersState) {
    if let Some(switcher) = state.switcher.as_mut() {
        if switcher.kind == Mode::Cycle {
            switcher.selected = overview::step(switcher.selected, switcher.windows.len(), forward);
        }
        return;
    }
    let Some(output) = current_output(state) else {
        return;
    };
    let windows = candidates(state, &output);
    if windows.len() < 2 {
        return;
    }
    let selected = if forward { 1 } else { windows.len() - 1 };
    state.switcher = Some(Switcher {
        kind: Mode::Cycle,
        output,
        windows,
        selected,
        hold_alt: modifiers.alt,
        hold_logo: modifiers.logo,
    });
}

pub fn toggle_overview(state: &mut Slate) {
    if state.switcher.is_some() {
        cancel(state);
        return;
    }
    let Some(output) = current_output(state) else {
        return;
    };
    let windows = candidates(state, &output);
    if windows.is_empty() {
        return;
    }
    state.switcher = Some(Switcher {
        kind: Mode::Overview,
        output,
        windows,
        selected: 0,
        hold_alt: false,
        hold_logo: false,
    });
}

pub fn cancel(state: &mut Slate) {
    state.switcher = None;
}

pub fn commit(state: &mut Slate) {
    let Some(switcher) = state.switcher.take() else {
        return;
    };
    if let Some(window) = switcher.windows.get(switcher.selected) {
        shell::activate_window(state, window);
    }
}

pub fn released(state: &Slate, modifiers: &ModifiersState) -> bool {
    state.switcher.as_ref().is_some_and(|switcher| {
        switcher.kind == Mode::Cycle
            && !(switcher.hold_alt && modifiers.alt)
            && !(switcher.hold_logo && modifiers.logo)
    })
}

pub fn step(state: &mut Slate, forward: bool) {
    if let Some(switcher) = state.switcher.as_mut() {
        switcher.selected = overview::step(switcher.selected, switcher.windows.len(), forward);
    }
}

pub fn navigate(state: &mut Slate, dx: i32, dy: i32) {
    if let Some(switcher) = state.switcher.as_mut() {
        switcher.selected = match switcher.kind {
            Mode::Overview => overview::move_in_grid(switcher.selected, switcher.windows.len(), dx, dy),
            Mode::Cycle => overview::step(switcher.selected, switcher.windows.len(), dx + dy > 0),
        };
    }
}

fn frame_of(state: &Slate, window: &Window) -> Option<Rectangle<i32, Logical>> {
    decoration::frame_rect(state, window).or_else(|| state.space.element_geometry(window))
}

fn tiles(state: &Slate, switcher: &Switcher) -> (Vec<Tile>, Option<Bounds>) {
    let Some(geometry) = state.space.output_geometry(&switcher.output) else {
        return (Vec::new(), None);
    };
    let mut zone = layer_map_for_output(&switcher.output).non_exclusive_zone();
    zone.loc += geometry.loc;
    let area = (zone.loc.x, zone.loc.y, zone.size.w, zone.size.h);

    let (cells, panel) = match switcher.kind {
        Mode::Overview => (overview::grid(switcher.windows.len(), area, GAP), None),
        Mode::Cycle => {
            let (cells, panel) = overview::row(switcher.windows.len(), area, ROW_CELL, GAP);
            (cells, Some(panel))
        }
    };

    let tiles = switcher
        .windows
        .iter()
        .zip(cells)
        .filter_map(|(window, cell)| {
            let frame = frame_of(state, window)?;
            let (scale, location) = overview::fit((frame.size.w, frame.size.h), cell);
            Some(Tile {
                window: window.clone(),
                cell,
                scale,
                target: location.into(),
                frame,
            })
        })
        .collect();
    (tiles, panel)
}

pub fn hit(state: &Slate, location: Point<f64, Logical>) -> Option<usize> {
    let switcher = state.switcher.as_ref()?;
    let (tiles, _) = tiles(state, switcher);
    tiles
        .iter()
        .position(|tile| overview::contains(tile.cell, location.x, location.y))
}

pub fn hover(state: &mut Slate, location: Point<f64, Logical>) {
    if let Some(index) = hit(state, location)
        && let Some(switcher) = state.switcher.as_mut()
    {
        switcher.selected = index;
    }
}

pub fn click(state: &mut Slate, location: Point<f64, Logical>) {
    match hit(state, location) {
        Some(index) => {
            if let Some(switcher) = state.switcher.as_mut() {
                switcher.selected = index;
            }
            commit(state);
        }
        None => cancel(state),
    }
}

fn solid(rect: Bounds, color: [f32; 4], origin: Point<i32, Logical>, scale: f64) -> OutputElement {
    let buffer = SolidColorBuffer::new((rect.2.max(1), rect.3.max(1)), color);
    let location = (Point::<i32, Logical>::from((rect.0, rect.1)) - origin).to_physical_precise_round(scale);
    OutputElement::Solid(SolidColorRenderElement::from_buffer(&buffer, location, scale, 1.0, Kind::Unspecified))
}

pub fn elements(state: &Slate, renderer: &mut GlesRenderer, output: &Output, elements: &mut Vec<OutputElement>) {
    let Some(switcher) = state.switcher.as_ref() else {
        return;
    };
    if &switcher.output != output {
        return;
    }
    let Some(geometry) = state.space.output_geometry(output) else {
        return;
    };
    let scale = output.current_scale().fractional_scale();
    let origin = geometry.loc;
    let (tiles, panel) = tiles(state, switcher);
    let accent = state.settings.border_focused;
    let accent = [
        ((accent >> 16) & 0xff) as f32 / 255.0,
        ((accent >> 8) & 0xff) as f32 / 255.0,
        (accent & 0xff) as f32 / 255.0,
        1.0,
    ];

    for (index, tile) in tiles.iter().enumerate() {
        if index == switcher.selected {
            let size = (
                (f64::from(tile.frame.size.w) * tile.scale).round() as i32,
                (f64::from(tile.frame.size.h) * tile.scale).round() as i32,
            );
            let rect = (tile.target.x - BORDER, tile.target.y - BORDER, size.0 + 2 * BORDER, size.1 + 2 * BORDER);
            for edge in [
                (rect.0, rect.1, rect.2, BORDER),
                (rect.0, rect.1 + rect.3 - BORDER, rect.2, BORDER),
                (rect.0, rect.1, BORDER, rect.3),
                (rect.0 + rect.2 - BORDER, rect.1, BORDER, rect.3),
            ] {
                elements.push(solid(edge, accent, origin, scale));
            }
        }
    }

    for tile in &tiles {
        let focused = false;
        let group = render::window_group(state, renderer, output, &tile.window, origin, scale, 1.0, Some(tile.frame), focused);
        let effect = Effect {
            alpha: 1.0,
            scale: (tile.scale, tile.scale),
            offset: (
                f64::from(tile.target.x - tile.frame.loc.x),
                f64::from(tile.target.y - tile.frame.loc.y),
            )
                .into(),
            anchor: Anchor::TopLeft,
        };
        elements.extend(render::wrap(group, &effect, tile.frame, origin, scale));
    }

    let backdrop = match panel {
        Some(panel) => (panel.0, panel.1, panel.2, panel.3),
        None => (geometry.loc.x, geometry.loc.y, geometry.size.w, geometry.size.h),
    };
    let alpha = if panel.is_some() { 0.78 } else { 0.72 };
    elements.push(solid(backdrop, [0.05 * alpha, 0.05 * alpha, 0.07 * alpha, alpha], origin, scale));
}
