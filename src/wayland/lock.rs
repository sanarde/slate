// SPDX-License-Identifier: GPL-3.0-only

use smithay::delegate_session_lock;
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker};

use super::focus::KeyFocus;
use super::shell;
use super::state::Slate;

pub fn surface_for(state: &Slate, output: &Output) -> Option<WlSurface> {
    state
        .lock
        .as_ref()?
        .iter()
        .find(|(locked, surface)| locked == output && surface.alive())
        .map(|(_, surface)| surface.wl_surface().clone())
}

pub fn prune(state: &mut Slate) {
    if let Some(lock) = state.lock.as_mut() {
        lock.retain(|(_, surface)| surface.alive());
    }
}

impl SessionLockHandler for Slate {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        self.lock = Some(Vec::new());
        self.seat
            .get_keyboard()
            .unwrap()
            .set_focus(self, None, SERIAL_COUNTER.next_serial());
        confirmation.lock();
    }

    fn unlock(&mut self) {
        self.lock = None;
        let top = self.space.elements().next_back().cloned();
        shell::focus_window(self, top.as_ref(), SERIAL_COUNTER.next_serial());
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        let Some(output) = Output::from_resource(&output) else {
            return;
        };
        let Some(geometry) = self.space.output_geometry(&output) else {
            return;
        };

        surface.with_pending_state(|pending| {
            pending.size = Some((geometry.size.w as u32, geometry.size.h as u32).into());
        });
        surface.send_configure();

        let focus = KeyFocus::Wayland(surface.wl_surface().clone());
        self.seat
            .get_keyboard()
            .unwrap()
            .set_focus(self, Some(focus), SERIAL_COUNTER.next_serial());
        if let Some(lock) = self.lock.as_mut() {
            lock.push((output, surface));
        }
    }
}

delegate_session_lock!(Slate);
