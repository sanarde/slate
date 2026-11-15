// SPDX-License-Identifier: GPL-3.0-only

use smithay::delegate_fractional_scale;
use smithay::desktop::layer_map_for_output;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::fractional_scale::{FractionalScaleHandler, with_fractional_scale};

use super::state::Slate;

impl FractionalScaleHandler for Slate {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self
            .space
            .outputs()
            .next()
            .map(|output| output.current_scale().fractional_scale())
            .unwrap_or(1.0);
        with_states(&surface, |states| {
            with_fractional_scale(states, |fractional| fractional.set_preferred_scale(scale));
        });
    }
}

pub fn update(state: &Slate) {
    let first = state.space.outputs().next().cloned();

    for window in state.space.elements().chain(state.parked.iter().map(|(window, _)| window)) {
        let output = state.space.outputs_for_element(window).into_iter().next().or_else(|| first.clone());
        let Some(output) = output else {
            continue;
        };
        let scale = output.current_scale().fractional_scale();
        window.with_surfaces(|_, states| {
            with_fractional_scale(states, |fractional| fractional.set_preferred_scale(scale));
        });
    }

    for output in state.space.outputs() {
        let scale = output.current_scale().fractional_scale();
        for layer in layer_map_for_output(output).layers() {
            layer.with_surfaces(|_, states| {
                with_fractional_scale(states, |fractional| fractional.set_preferred_scale(scale));
            });
        }
    }
}

delegate_fractional_scale!(Slate);
