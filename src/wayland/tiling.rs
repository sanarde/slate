// SPDX-License-Identifier: GPL-3.0-only

use smithay::desktop::{Window, layer_map_for_output};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::utils::Rectangle;
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::SurfaceCachedState;

use super::animation;
use super::decoration;
use super::shell::{self, with_window_state, workspace_of};
use super::state::Slate;
use crate::layout::{self, Area, Layout, Params, WorkspaceLayout};

const RATIO_STEP: f32 = 0.05;
const MAX_MASTERS: usize = 8;

pub fn is_tiled(window: &Window) -> bool {
    with_window_state(window, |window_state| window_state.tiled)
}

fn eligible(window: &Window) -> bool {
    !shell::is_override_redirect(window)
        && with_window_state(window, |window_state| {
            !window_state.floating && !window_state.maximized && !window_state.fullscreen
        })
}

pub fn decide_floating(state: &Slate, window: &Window) -> bool {
    let id = decoration::app_id(window);
    if state.settings.tiling.floating_rules.contains(&id) {
        return true;
    }

    if let Some(toplevel) = window.toplevel() {
        if toplevel.parent().is_some() {
            return true;
        }
        let (min, max) = with_states(toplevel.wl_surface(), |states| {
            let mut cached = states.cached_state.get::<SurfaceCachedState>();
            let current = cached.current();
            (current.min_size, current.max_size)
        });
        return min.w > 0 && min.h > 0 && min == max;
    }

    if let Some(surface) = window.x11_surface() {
        if surface.is_transient_for().is_some() {
            return true;
        }
        if let (Some(min), Some(max)) = (surface.min_size(), surface.max_size()) {
            return min.w > 0 && min.h > 0 && min == max;
        }
    }
    false
}

fn set_tiled_states(window: &Window, tiled: bool) {
    let Some(toplevel) = window.toplevel() else {
        return;
    };
    toplevel.with_pending_state(|pending| {
        for flag in [
            ToplevelState::TiledLeft,
            ToplevelState::TiledRight,
            ToplevelState::TiledTop,
            ToplevelState::TiledBottom,
        ] {
            if tiled {
                pending.states.set(flag);
            } else {
                pending.states.unset(flag);
            }
        }
    });
}

fn apply_tile(state: &mut Slate, window: &Window, frame: Area) {
    let insets = decoration::insets_with(state, window, false, false);
    let content = Rectangle::new(
        (frame.x + insets.left, frame.y + insets.top).into(),
        (
            (frame.w - insets.left - insets.right).max(1),
            (frame.h - insets.top - insets.bottom).max(1),
        )
            .into(),
    );

    let from = decoration::frame_rect(state, window);
    if !is_tiled(window) {
        let current = state.space.element_geometry(window);
        with_window_state(window, |window_state| {
            window_state.float_rect = current;
            window_state.tiled = true;
        });
    }

    set_tiled_states(window, true);
    if let Some(toplevel) = window.toplevel() {
        toplevel.with_pending_state(|pending| pending.size = Some(content.size));
        toplevel.send_pending_configure();
    } else if let Some(surface) = window.x11_surface() {
        let _ = surface.configure(content);
    }
    state.space.map_element(window.clone(), content.loc, false);

    if let Some(from) = from {
        let to = Rectangle::new((frame.x, frame.y).into(), (frame.w, frame.h).into());
        animation::start_geometry(state, window, from, to);
    }
}

fn release(state: &mut Slate, window: &Window) {
    let from = decoration::frame_rect(state, window);
    let rect = with_window_state(window, |window_state| {
        window_state.tiled = false;
        window_state.float_rect.take()
    });
    set_tiled_states(window, false);

    let Some(rect) = rect else {
        if let Some(toplevel) = window.toplevel() {
            toplevel.send_pending_configure();
        }
        return;
    };
    if let Some(toplevel) = window.toplevel() {
        toplevel.with_pending_state(|pending| pending.size = Some(rect.size));
        toplevel.send_pending_configure();
    } else if let Some(surface) = window.x11_surface() {
        let _ = surface.configure(rect);
    }
    state.space.map_element(window.clone(), rect.loc, false);

    if let Some(from) = from {
        let to = decoration::frame_of(rect, decoration::insets(state, window));
        animation::start_geometry(state, window, from, to);
    }
}

fn workspace_windows(state: &Slate, workspace: usize) -> Vec<Window> {
    state
        .tile_order
        .iter()
        .filter(|window| state.space.element_location(window).is_some() && workspace_of(window) == workspace)
        .cloned()
        .collect()
}

pub fn arrange(state: &mut Slate) {
    let workspace = state.workspace;
    let current = state.workspace_layouts[workspace];
    let windows = workspace_windows(state, workspace);

    for window in windows.iter().filter(|window| !eligible(window) && is_tiled(window)) {
        with_window_state(window, |window_state| {
            window_state.tiled = false;
            window_state.float_rect = None;
        });
        set_tiled_states(window, false);
    }

    let tiling = &state.settings.tiling;
    let params = Params {
        gap: tiling.gap,
        outer_gap: tiling.outer_gap,
        master_ratio: current.ratio,
        master_count: current.masters,
    };

    if !current.layout.tiles() {
        for window in windows.iter().filter(|window| is_tiled(window)) {
            release(state, window);
        }
        return;
    }

    let focused = shell::focused_window(state);
    let outputs: Vec<_> = state.space.outputs().cloned().collect();
    for output in outputs {
        let Some(geometry) = state.space.output_geometry(&output) else {
            continue;
        };
        let mut zone = layer_map_for_output(&output).non_exclusive_zone();
        zone.loc += geometry.loc;
        let area = Area {
            x: zone.loc.x,
            y: zone.loc.y,
            w: zone.size.w,
            h: zone.size.h,
        };

        let mut group: Vec<Window> = windows
            .iter()
            .filter(|window| eligible(window) && shell::output_for_window(state, window).as_ref() == Some(&output))
            .cloned()
            .collect();
        let tiles = layout::arrange(current.layout, area, group.len(), &params);

        let mut pairs: Vec<(Window, Area)> = group.drain(..).zip(tiles).collect();
        pairs.sort_by_key(|(window, _)| focused.as_ref() == Some(window));
        for (window, tile) in pairs {
            apply_tile(state, &window, tile);
        }
    }
}

pub fn register(state: &mut Slate, window: &Window) {
    let floating = decide_floating(state, window);
    with_window_state(window, |window_state| window_state.floating = floating);
    if !state.tile_order.contains(window) {
        state.tile_order.push(window.clone());
    }
}

pub fn forget(state: &mut Slate, window: &Window) {
    state.tile_order.retain(|known| known != window);
}

pub fn toggle_tiling(state: &mut Slate) {
    let workspace = state.workspace;
    let default = state.settings.tiling.layout;
    let entry = &mut state.workspace_layouts[workspace];
    entry.layout = if entry.layout.tiles() {
        Layout::Floating
    } else if default.tiles() {
        default
    } else {
        Layout::MasterStack
    };
    arrange(state);
}

pub fn layout_next(state: &mut Slate) {
    let workspace = state.workspace;
    let entry = &mut state.workspace_layouts[workspace];
    entry.layout = entry.layout.next();
    arrange(state);
}

pub fn toggle_floating(state: &mut Slate, window: &Window) {
    let floating = with_window_state(window, |window_state| {
        window_state.floating = !window_state.floating;
        window_state.floating
    });
    if floating {
        release(state, window);
    }
    arrange(state);
}

pub fn make_floating(state: &mut Slate, window: &Window) {
    if !is_tiled(window) {
        return;
    }
    with_window_state(window, |window_state| window_state.floating = true);
    release(state, window);
    arrange(state);
}

pub fn master_ratio(state: &mut Slate, grow: bool) {
    let workspace = state.workspace;
    let entry = &mut state.workspace_layouts[workspace];
    let step = if grow { RATIO_STEP } else { -RATIO_STEP };
    entry.ratio = (entry.ratio + step).clamp(0.1, 0.9);
    arrange(state);
}

pub fn master_count(state: &mut Slate, more: bool) {
    let workspace = state.workspace;
    let entry = &mut state.workspace_layouts[workspace];
    entry.masters = if more {
        (entry.masters + 1).min(MAX_MASTERS)
    } else {
        entry.masters.saturating_sub(1).max(1)
    };
    arrange(state);
}

fn position_of(state: &Slate, window: &Window) -> Option<usize> {
    state.tile_order.iter().position(|known| known == window)
}

pub fn promote(state: &mut Slate, window: &Window) {
    let workspace = workspace_of(window);
    let siblings = workspace_windows(state, workspace);
    let Some(first) = siblings.iter().find(|candidate| eligible(candidate)).cloned() else {
        return;
    };

    let target = if &first == window {
        siblings.iter().filter(|candidate| eligible(candidate)).nth(1).cloned()
    } else {
        Some(first)
    };
    if let Some(other) = target {
        swap(state, window, &other);
    }
}

fn swap(state: &mut Slate, a: &Window, b: &Window) {
    if let (Some(first), Some(second)) = (position_of(state, a), position_of(state, b)) {
        state.tile_order.swap(first, second);
        arrange(state);
    }
}

pub fn swap_neighbor(state: &mut Slate, window: &Window, forward: bool) {
    let siblings: Vec<Window> = workspace_windows(state, workspace_of(window))
        .into_iter()
        .filter(eligible)
        .collect();
    let Some(index) = siblings.iter().position(|candidate| candidate == window) else {
        return;
    };
    let count = siblings.len();
    if count < 2 {
        return;
    }
    let other = if forward {
        &siblings[(index + 1) % count]
    } else {
        &siblings[(index + count - 1) % count]
    };
    swap(state, window, &other.clone());
}

pub fn reset_layouts(state: &mut Slate) {
    let tiling = state.settings.tiling.clone();
    for entry in &mut state.workspace_layouts {
        *entry = WorkspaceLayout::new(&tiling);
    }
}
