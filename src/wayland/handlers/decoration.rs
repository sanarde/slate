// SPDX-License-Identifier: GPL-3.0-only

use smithay::delegate_xdg_decoration;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::xdg::ToplevelSurface;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;

use super::super::decoration::{app_id, negotiated_mode};
use super::super::shell;
use super::super::state::Slate;

pub fn apply_policy(state: &mut Slate, toplevel: &ToplevelSurface, requested: Option<Mode>) {
    let id = shell::window_for_surface(state, toplevel.wl_surface())
        .map(|window| app_id(&window))
        .unwrap_or_default();
    let mode = negotiated_mode(state, &id, requested);

    toplevel.with_pending_state(|pending| pending.decoration_mode = Some(mode));
    if toplevel.is_initial_configure_sent() {
        toplevel.send_pending_configure();
    }
}

pub fn reapply(state: &mut Slate, toplevel: &ToplevelSurface) {
    let current = toplevel.with_pending_state(|pending| pending.decoration_mode);
    if current.is_some() {
        apply_policy(state, toplevel, current);
    }
}

impl XdgDecorationHandler for Slate {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        apply_policy(self, &toplevel, None);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        apply_policy(self, &toplevel, Some(mode));
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        apply_policy(self, &toplevel, None);
    }
}

delegate_xdg_decoration!(Slate);
