// SPDX-License-Identifier: GPL-3.0-only

use std::cell::RefCell;

use smithay::desktop::{
    PopupKind, Window, WindowSurfaceType, find_popup_root_surface, get_popup_toplevel_coords, layer_map_for_output,
};
use smithay::output::Output;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Serial};
use smithay::wayland::compositor::{get_parent, with_states};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::wlr_layer::LayerSurfaceData;
use smithay::wayland::shell::xdg::PopupSurface;
use smithay::xwayland::X11Surface;

use super::focus::KeyFocus;
use super::decoration;
use super::grabs;
use super::scale;
use super::tiling;
use super::state::Slate;

#[derive(Default)]
pub(super) struct WindowState {
    pub(super) restore: Option<Rectangle<i32, Logical>>,
    pub(super) maximized: bool,
    pub(super) fullscreen: bool,
    pub(super) workspace: usize,
    pub(super) floating: bool,
    pub(super) tiled: bool,
    pub(super) float_rect: Option<Rectangle<i32, Logical>>,
}

pub fn window_for_surface(state: &Slate, surface: &WlSurface) -> Option<Window> {
    state
        .space
        .elements()
        .chain(state.pending_windows.iter())
        .chain(state.parked.iter().map(|(window, _)| window))
        .chain(state.minimized.iter().map(|(window, _)| window))
        .find(|window| window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == surface))
        .cloned()
}

pub fn window_for_x11(state: &Slate, surface: &X11Surface) -> Option<Window> {
    state
        .space
        .elements()
        .chain(state.parked.iter().map(|(window, _)| window))
        .chain(state.minimized.iter().map(|(window, _)| window))
        .find(|window| window.x11_surface() == Some(surface))
        .cloned()
}

pub fn map_window(state: &mut Slate, window: Window) {
    if window.is_wayland() {
        state.pending_windows.push(window);
        return;
    }
    place_window(state, window);
}

fn place_window(state: &mut Slate, window: Window) {
    let workspace = state.workspace;
    with_window_state(&window, |window_state| window_state.workspace = workspace);
    let size = window.geometry().size;
    let insets = decoration::insets(state, &window);
    let frame_width = size.w + insets.left + insets.right;
    let frame_height = size.h + insets.top + insets.bottom;
    let zone = active_zone(state);
    let location = Point::from((
        zone.loc.x + ((zone.size.w - frame_width) / 2).max(0) + insets.left,
        zone.loc.y + ((zone.size.h - frame_height) / 2).max(0) + insets.top,
    ));

    if let Some(surface) = window.x11_surface() {
        let _ = surface.configure(Rectangle::new(location, size));
    }
    tiling::register(state, &window);
    state.space.map_element(window.clone(), location, false);
    scale::update(state);
    focus_window(state, Some(&window), SERIAL_COUNTER.next_serial());
    tiling::arrange(state);
}

pub fn unmap_window(state: &mut Slate, window: &Window) {
    state.pending_windows.retain(|pending| pending != window);
    state.parked.retain(|(parked, _)| parked != window);
    state.minimized.retain(|(minimized, _)| minimized != window);
    state.space.unmap_elem(window);
    grabs::forget_window(state, window);
    tiling::forget(state, window);

    let top = state.space.elements().next_back().cloned();
    focus_window(state, top.as_ref(), SERIAL_COUNTER.next_serial());
    tiling::arrange(state);
}

pub fn focus_window(state: &mut Slate, window: Option<&Window>, serial: Serial) {
    if state.lock.is_some() {
        return;
    }
    if let Some(window) = window {
        state.space.raise_element(window, true);
        if let (Some(surface), Some(xwm)) = (window.x11_surface(), state.xwm.as_mut()) {
            let _ = xwm.raise_window(surface);
        }
    }

    for element in state.space.elements() {
        element.set_activated(Some(element) == window);
        if let Some(toplevel) = element.toplevel() {
            toplevel.send_pending_configure();
        }
    }

    let focus = window.and_then(KeyFocus::from_window);
    let keyboard = state.seat.get_keyboard().unwrap();
    keyboard.set_focus(state, focus, serial);
}

pub(super) fn is_override_redirect(window: &Window) -> bool {
    window
        .x11_surface()
        .is_some_and(|surface| surface.is_override_redirect())
}

pub(super) fn workspace_of(window: &Window) -> usize {
    with_window_state(window, |window_state| window_state.workspace)
}

pub fn switch_workspace(state: &mut Slate, target: usize) {
    if target >= state.settings.workspace_count || target == state.workspace {
        return;
    }

    let leaving: Vec<Window> = state
        .space
        .elements()
        .filter(|window| !is_override_redirect(window))
        .cloned()
        .collect();
    for window in leaving {
        if let Some(location) = state.space.element_location(&window) {
            state.parked.push((window.clone(), location));
        }
        state.space.unmap_elem(&window);
    }

    state.workspace = target;
    let arriving: Vec<(Window, Point<i32, Logical>)> = state
        .parked
        .iter()
        .filter(|(window, _)| workspace_of(window) == target)
        .cloned()
        .collect();
    state.parked.retain(|(window, _)| workspace_of(window) != target);
    for (window, location) in arriving {
        state.space.map_element(window, location, false);
    }

    let top = state.space.elements().next_back().cloned();
    focus_window(state, top.as_ref(), SERIAL_COUNTER.next_serial());
    tiling::arrange(state);
}

pub fn clamp_workspaces(state: &mut Slate) {
    let last = state.settings.workspace_count.saturating_sub(1);
    if state.workspace > last {
        switch_workspace(state, last);
    }

    let stranded: Vec<Window> = state
        .parked
        .iter()
        .filter(|(window, _)| workspace_of(window) > last)
        .map(|(window, _)| window.clone())
        .collect();
    for window in &stranded {
        with_window_state(window, |window_state| window_state.workspace = last);
    }
    if last != state.workspace {
        return;
    }

    let arriving: Vec<(Window, Point<i32, Logical>)> = state
        .parked
        .iter()
        .filter(|(window, _)| stranded.contains(window))
        .cloned()
        .collect();
    state.parked.retain(|(window, _)| !stranded.contains(window));
    for (window, location) in arriving {
        state.space.map_element(window, location, false);
    }
}

pub fn move_to_workspace(state: &mut Slate, window: &Window, target: usize) {
    if target >= state.settings.workspace_count {
        return;
    }
    with_window_state(window, |window_state| window_state.workspace = target);
    if target == state.workspace {
        return;
    }

    if let Some(location) = state.space.element_location(window) {
        state.parked.push((window.clone(), location));
    }
    state.space.unmap_elem(window);

    let top = state.space.elements().next_back().cloned();
    focus_window(state, top.as_ref(), SERIAL_COUNTER.next_serial());
    tiling::arrange(state);
}

pub fn minimize_window(state: &mut Slate, window: &Window) {
    if state.minimized.iter().any(|(minimized, _)| minimized == window) {
        return;
    }
    let Some(location) = state.space.element_location(window) else {
        return;
    };

    state.minimized.push((window.clone(), location));
    state.space.unmap_elem(window);
    grabs::forget_window(state, window);
    if state.decoration_hover.as_ref().is_some_and(|(hovered, _)| hovered == window) {
        state.decoration_hover = None;
    }
    window.set_activated(false);
    if let Some(toplevel) = window.toplevel() {
        toplevel.send_pending_configure();
    }

    let top = state.space.elements().next_back().cloned();
    focus_window(state, top.as_ref(), SERIAL_COUNTER.next_serial());
    tiling::arrange(state);
}

pub fn restore_window(state: &mut Slate, window: &Window) {
    let Some(index) = state.minimized.iter().position(|(minimized, _)| minimized == window) else {
        return;
    };
    let (window, location) = state.minimized.remove(index);

    let workspace = workspace_of(&window);
    if workspace != state.workspace {
        switch_workspace(state, workspace);
    }
    state.space.map_element(window.clone(), location, true);
    scale::update(state);
    focus_window(state, Some(&window), SERIAL_COUNTER.next_serial());
    tiling::arrange(state);
}

pub fn restore_last_minimized(state: &mut Slate) {
    if let Some((window, _)) = state.minimized.last().cloned() {
        restore_window(state, &window);
    }
}

pub fn activate_surface(state: &mut Slate, surface: &WlSurface) {
    let minimized = state
        .minimized
        .iter()
        .map(|(window, _)| window)
        .find(|window| window.wl_surface().as_deref() == Some(surface))
        .cloned();
    if let Some(window) = minimized {
        restore_window(state, &window);
        return;
    }

    let window = state
        .space
        .elements()
        .chain(state.parked.iter().map(|(window, _)| window))
        .find(|window| window.wl_surface().as_deref() == Some(surface))
        .cloned();
    let Some(window) = window else {
        return;
    };

    let workspace = workspace_of(&window);
    if workspace != state.workspace {
        switch_workspace(state, workspace);
    }
    focus_window(state, Some(&window), SERIAL_COUNTER.next_serial());
}

pub fn cycle_focus(state: &mut Slate, forward: bool) {
    let windows: Vec<Window> = state
        .space
        .elements()
        .filter(|window| !is_override_redirect(window))
        .cloned()
        .collect();
    if windows.len() < 2 {
        return;
    }

    let target = if forward {
        windows[0].clone()
    } else {
        for window in &windows[..windows.len() - 1] {
            state.space.raise_element(window, false);
        }
        windows[windows.len() - 2].clone()
    };
    focus_window(state, Some(&target), SERIAL_COUNTER.next_serial());
}

pub fn focused_window(state: &Slate) -> Option<Window> {
    let focus = state.seat.get_keyboard()?.current_focus()?;
    state
        .space
        .elements()
        .find(|window| KeyFocus::from_window(window).as_ref() == Some(&focus))
        .cloned()
}

pub fn close_window(window: &Window) {
    if let Some(toplevel) = window.toplevel() {
        toplevel.send_close();
    } else if let Some(surface) = window.x11_surface() {
        let _ = surface.close();
    }
}

pub fn toggle_maximized(state: &mut Slate, window: &Window) {
    let maximized = with_window_state(window, |window_state| window_state.maximized);
    set_maximized(state, window, !maximized);
}

pub fn toggle_fullscreen(state: &mut Slate, window: &Window) {
    let fullscreen = with_window_state(window, |window_state| window_state.fullscreen);
    set_fullscreen(state, window, !fullscreen);
}

pub fn flags(window: &Window) -> (bool, bool) {
    with_window_state(window, |window_state| (window_state.maximized, window_state.fullscreen))
}

fn inset_target(state: &Slate, window: &Window, target: Rectangle<i32, Logical>, maximized: bool) -> Rectangle<i32, Logical> {
    let insets = decoration::insets_with(state, window, maximized, false);
    Rectangle::new(
        (target.loc.x + insets.left, target.loc.y + insets.top).into(),
        (
            (target.size.w - insets.left - insets.right).max(1),
            (target.size.h - insets.top - insets.bottom).max(1),
        )
            .into(),
    )
}

pub fn refit_maximized(state: &mut Slate) {
    let windows: Vec<Window> = state
        .space
        .elements()
        .filter(|window| flags(window).0)
        .cloned()
        .collect();
    for window in windows {
        let Some(output) = output_for_window(state, &window) else {
            continue;
        };
        let mut target = layer_map_for_output(&output).non_exclusive_zone();
        target.loc += state.space.output_geometry(&output).unwrap().loc;
        let target = inset_target(state, &window, target, true);

        let restore = with_window_state(&window, |window_state| window_state.restore);
        resize_to_state(state, &window, ToplevelState::Maximized, true, target);
        with_window_state(&window, |window_state| window_state.restore = restore);
    }
}

pub fn set_maximized(state: &mut Slate, window: &Window, maximized: bool) {
    let target = output_for_window(state, window).map(|output| layer_map_for_output(&output).non_exclusive_zone());
    let Some(mut target) = target else {
        return;
    };
    if let Some(output) = output_for_window(state, window) {
        target.loc += state.space.output_geometry(&output).unwrap().loc;
    }

    let applied = with_window_state(window, |window_state| {
        if window_state.maximized == maximized || window_state.fullscreen {
            return false;
        }
        window_state.maximized = maximized;
        true
    });
    if !applied {
        send_current_state(window);
        return;
    }

    if let Some(surface) = window.x11_surface() {
        let _ = surface.set_maximized(maximized);
    }
    let target = inset_target(state, window, target, maximized);
    resize_to_state(state, window, ToplevelState::Maximized, maximized, target);
    tiling::arrange(state);
}

pub fn set_fullscreen(state: &mut Slate, window: &Window, fullscreen: bool) {
    let Some(output) = output_for_window(state, window) else {
        return;
    };
    let target = state.space.output_geometry(&output).unwrap();

    let applied = with_window_state(window, |window_state| {
        if window_state.fullscreen == fullscreen || window_state.maximized {
            return false;
        }
        window_state.fullscreen = fullscreen;
        true
    });
    if !applied {
        send_current_state(window);
        return;
    }

    if let Some(surface) = window.x11_surface() {
        let _ = surface.set_fullscreen(fullscreen);
    }
    resize_to_state(state, window, ToplevelState::Fullscreen, fullscreen, target);
    tiling::arrange(state);
}

fn resize_to_state(
    state: &mut Slate,
    window: &Window,
    toplevel_state: ToplevelState,
    enabled: bool,
    target: Rectangle<i32, Logical>,
) {
    let current = state.space.element_geometry(window);
    let geometry = if enabled {
        with_window_state(window, |window_state| window_state.restore = current);
        target
    } else {
        let restore = with_window_state(window, |window_state| window_state.restore.take());
        restore.or(current).unwrap_or(target)
    };

    if let Some(toplevel) = window.toplevel() {
        toplevel.with_pending_state(|pending| {
            if enabled {
                pending.states.set(toplevel_state);
                pending.size = Some(geometry.size);
            } else {
                pending.states.unset(toplevel_state);
                pending.size = Some(geometry.size);
            }
        });
        toplevel.send_pending_configure();
    } else if let Some(surface) = window.x11_surface() {
        let _ = surface.configure(geometry);
    }

    state.space.map_element(window.clone(), geometry.loc, true);
}

fn send_current_state(window: &Window) {
    if let Some(toplevel) = window.toplevel() {
        toplevel.send_configure();
    }
}

pub(super) fn with_window_state<T>(window: &Window, f: impl FnOnce(&mut WindowState) -> T) -> T {
    window.user_data().insert_if_missing(|| RefCell::new(WindowState::default()));
    let mut window_state = window.user_data().get::<RefCell<WindowState>>().unwrap().borrow_mut();
    f(&mut window_state)
}

pub(super) fn output_for_window(state: &Slate, window: &Window) -> Option<Output> {
    state
        .space
        .outputs_for_element(window)
        .into_iter()
        .next()
        .or_else(|| state.space.outputs().next().cloned())
}

pub(super) fn active_zone(state: &Slate) -> Rectangle<i32, Logical> {
    let pointer = state.seat.get_pointer().unwrap().current_location();
    let output = state
        .space
        .output_under(pointer)
        .next()
        .or_else(|| state.space.outputs().next())
        .cloned();
    let Some(output) = output else {
        return Rectangle::default();
    };

    let mut zone = layer_map_for_output(&output).non_exclusive_zone();
    zone.loc += state.space.output_geometry(&output).unwrap().loc;
    zone
}

pub fn unconstrain_popup(state: &Slate, popup: &PopupSurface) {
    let kind = PopupKind::Xdg(popup.clone());
    let Ok(root) = find_popup_root_surface(&kind) else {
        return;
    };
    let Some(window) = window_for_surface(state, &root) else {
        return;
    };
    let Some(window_geometry) = state.space.element_geometry(&window) else {
        return;
    };
    let Some(output) = output_for_window(state, &window) else {
        return;
    };

    let mut target = state.space.output_geometry(&output).unwrap();
    target.loc -= get_popup_toplevel_coords(&kind);
    target.loc -= window_geometry.loc;

    popup.with_pending_state(|pending| {
        pending.geometry = pending.positioner.get_unconstrained_geometry(target);
    });
}

pub fn handle_commit(state: &mut Slate, surface: &WlSurface) {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }

    if let Some(index) = state
        .pending_windows
        .iter()
        .position(|window| window.toplevel().is_some_and(|toplevel| toplevel.wl_surface() == &root))
    {
        let window = state.pending_windows[index].clone();
        window.on_commit();
        let toplevel = window.toplevel().unwrap();
        if !toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        } else if !window.geometry().size.is_empty() {
            state.pending_windows.remove(index);
            place_window(state, window);
        }
        return;
    }

    let mapped = state
        .space
        .elements()
        .find(|window| window.wl_surface().as_deref() == Some(&root))
        .cloned();
    if let Some(window) = mapped {
        window.on_commit();
        grabs::handle_resize_commit(state, &window);
    }

    state.popups.commit(surface);
    if let Some(PopupKind::Xdg(popup)) = state.popups.find_popup(surface)
        && !popup.is_initial_configure_sent()
    {
        let _ = popup.send_configure();
    }

    handle_layer_commit(state, surface);
}

fn handle_layer_commit(state: &mut Slate, surface: &WlSurface) {
    let output = state
        .space
        .outputs()
        .find(|output| {
            layer_map_for_output(output)
                .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
                .is_some()
        })
        .cloned();
    let Some(output) = output else {
        return;
    };

    let mut map = layer_map_for_output(&output);
    map.arrange();
    let layer = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).cloned().unwrap();
    drop(map);

    let initial_configure_sent = with_states(surface, |states| {
        states
            .data_map
            .get::<LayerSurfaceData>()
            .unwrap()
            .lock()
            .unwrap()
            .initial_configure_sent
    });
    if !initial_configure_sent {
        layer.layer_surface().send_configure();
    }
}
