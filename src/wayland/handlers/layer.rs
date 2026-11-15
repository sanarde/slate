// SPDX-License-Identifier: GPL-3.0-only

use smithay::delegate_layer_shell;
use smithay::desktop::{LayerSurface, PopupKind, layer_map_for_output};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::wayland::shell::wlr_layer::{self, Layer, WlrLayerShellHandler, WlrLayerShellState};
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
    }
}

delegate_layer_shell!(Slate);
