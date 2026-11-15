// SPDX-License-Identifier: GPL-3.0-only

use smithay::delegate_xdg_shell;
use smithay::desktop::{PopupKind, PopupKeyboardGrab, PopupPointerGrab, PopupUngrabStrategy, Window, find_popup_root_surface};
use smithay::input::Seat;
use smithay::input::pointer::Focus;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::utils::Serial;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};

use super::super::focus::KeyFocus;
use super::super::grabs;
use super::super::shell;
use super::super::state::Slate;

impl XdgShellHandler for Slate {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        shell::map_window(self, Window::new_wayland_window(surface));
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        surface.with_pending_state(|state| state.geometry = positioner.get_geometry());
        shell::unconstrain_popup(self, &surface);
        if let Err(error) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            tracing::warn!(%error, "failed to track popup");
        }
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        shell::unconstrain_popup(self, &surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: WlSeat, serial: Serial) {
        let Some(seat) = Seat::<Slate>::from_resource(&seat) else {
            return;
        };
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface()) {
            grabs::start_move(self, &seat, window, serial, None);
        }
    }

    fn resize_request(&mut self, surface: ToplevelSurface, seat: WlSeat, serial: Serial, edges: ResizeEdge) {
        let Some(seat) = Seat::<Slate>::from_resource(&seat) else {
            return;
        };
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface()) {
            grabs::start_resize(self, &seat, window, edges.into(), serial, None);
        }
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface()) {
            shell::set_maximized(self, &window, true);
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface()) {
            shell::set_maximized(self, &window, false);
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface()) {
            shell::set_fullscreen(self, &window, true);
        }
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface()) {
            shell::minimize_window(self, &window);
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface()) {
            shell::set_fullscreen(self, &window, false);
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface())
            && shell::window_id(&window) != 0
        {
            super::super::ipc::window_event(self, "window-title", &window);
            super::super::foreign_toplevel::refresh(self);
        }
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        super::decoration::reapply(self, &surface);
        super::super::foreign_toplevel::refresh(self);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(window) = shell::window_for_surface(self, surface.wl_surface()) {
            shell::unmap_window(self, &window);
        }
    }

    fn grab(&mut self, surface: PopupSurface, seat: WlSeat, serial: Serial) {
        let Some(seat) = Seat::<Slate>::from_resource(&seat) else {
            return;
        };
        let popup = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&popup) else {
            return;
        };

        let Ok(mut grab) = self.popups.grab_popup(KeyFocus::Wayland(root), popup, &seat, serial) else {
            return;
        };

        let keyboard = seat.get_keyboard().unwrap();
        let pointer = seat.get_pointer().unwrap();

        if keyboard.is_grabbed() && !(keyboard.has_grab(serial) || grab.previous_serial().is_some_and(|previous| keyboard.has_grab(previous))) {
            grab.ungrab(PopupUngrabStrategy::All);
            return;
        }
        keyboard.set_focus(self, grab.current_grab(), serial);
        keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);

        if pointer.is_grabbed() && !(pointer.has_grab(serial) || grab.previous_serial().is_none_or(|previous| pointer.has_grab(previous))) {
            grab.ungrab(PopupUngrabStrategy::All);
            return;
        }
        pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
    }
}

delegate_xdg_shell!(Slate);
