// SPDX-License-Identifier: GPL-3.0-only

use smithay::delegate_layer_shell;
use smithay::desktop::{LayerSurface, PopupKind, layer_map_for_output};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::desktop::WindowSurfaceType;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor;
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::wlr_layer::{self, Anchor, Layer, LayerSurfaceAttributes, LayerSurfaceCachedState, WlrLayerShellHandler, WlrLayerShellState};
use std::sync::Mutex;
use smithay::wayland::shell::xdg::PopupSurface;

use super::super::state::Slate;

impl WlrLayerShellHandler for Slate {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: wlr_layer::LayerSurface,
        output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        let output = output
            .as_ref()
            .and_then(Output::from_resource)
            .or_else(|| self.space.outputs().next().cloned());
        let Some(output) = output else {
            surface.send_close();
            return;
        };

        let mut map = layer_map_for_output(&output);
        if let Err(error) = map.map_layer(&LayerSurface::new(surface, namespace)) {
            tracing::warn!(%error, "failed to map layer surface");
        }
        drop(map);
        super::super::scale::update(self);
    }

    fn new_popup(&mut self, _parent: wlr_layer::LayerSurface, popup: PopupSurface) {
        if let Err(error) = self.popups.track_popup(PopupKind::Xdg(popup)) {
            tracing::warn!(%error, "failed to track popup");
        }
    }

    fn layer_destroyed(&mut self, surface: wlr_layer::LayerSurface) {
        for output in self.space.outputs() {
            let mut map = layer_map_for_output(output);
            let layer = map.layer_for_surface(surface.wl_surface(), smithay::desktop::WindowSurfaceType::TOPLEVEL).cloned();
            if let Some(layer) = layer {
                map.unmap_layer(&layer);
            }
        }
        let keyboard = self.seat.get_keyboard().unwrap();
        let held = keyboard
            .current_focus()
            .is_some_and(|focus| focus.wl_surface().as_deref() == Some(surface.wl_surface()));
        if held && self.lock.is_none() {
            let top = self.space.elements().next_back().cloned();
            super::super::shell::focus_window(self, top.as_ref(), smithay::utils::SERIAL_COUNTER.next_serial());
        }
    }
}

pub fn guard_destroyed_role(surface: &WlSurface) {
    compositor::add_pre_commit_hook::<Slate, _>(surface, |state, _display, surface| {
        let had_role = compositor::with_states(surface, |states| states.data_map.get::<Mutex<LayerSurfaceAttributes>>().is_some());
        if !had_role {
            return;
        }
        let mapped = state.space.outputs().any(|output| {
            layer_map_for_output(output).layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).is_some()
        });
        if mapped {
            return;
        }
        compositor::with_states(surface, |states| {
            let mut cached = states.cached_state.get::<LayerSurfaceCachedState>();
            let pending = cached.pending();
            if pending.size.w == 0 && pending.size.h == 0 && pending.anchor.is_empty() {
                pending.anchor = Anchor::all();
            }
        });
    });
}

delegate_layer_shell!(Slate);
