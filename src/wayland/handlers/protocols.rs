// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::idle_notify::{IdleNotifierHandler, IdleNotifierState};
use smithay::wayland::tablet_manager::TabletSeatHandler;
use smithay::wayland::selection::primary_selection::{PrimarySelectionHandler, PrimarySelectionState};
use smithay::wayland::xdg_activation::{XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData};
use smithay::{
    delegate_cursor_shape, delegate_idle_notify, delegate_pointer_gestures, delegate_primary_selection,
    delegate_relative_pointer, delegate_single_pixel_buffer, delegate_viewporter, delegate_xdg_activation,
};

use super::super::shell;
use super::super::state::Slate;

const ACTIVATION_TOKEN_LIFETIME: Duration = Duration::from_secs(10);

impl PrimarySelectionHandler for Slate {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}

impl IdleNotifierHandler for Slate {
    fn idle_notifier_state(&mut self) -> &mut IdleNotifierState<Slate> {
        &mut self.idle_notifier_state
    }
}

impl XdgActivationHandler for Slate {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.activation_state
    }

    fn request_activation(&mut self, token: XdgActivationToken, token_data: XdgActivationTokenData, surface: WlSurface) {
        if token_data.timestamp.elapsed() <= ACTIVATION_TOKEN_LIFETIME {
            shell::activate_surface(self, &surface);
        }
        self.activation_state.remove_token(&token);
    }
}

impl TabletSeatHandler for Slate {}

delegate_primary_selection!(Slate);
delegate_idle_notify!(Slate);
delegate_xdg_activation!(Slate);
delegate_viewporter!(Slate);
delegate_cursor_shape!(Slate);
delegate_relative_pointer!(Slate);
delegate_single_pixel_buffer!(Slate);
delegate_pointer_gestures!(Slate);
