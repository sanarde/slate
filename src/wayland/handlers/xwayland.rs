// SPDX-License-Identifier: GPL-3.0-only

use std::os::fd::OwnedFd;
use std::process::Stdio;

use smithay::delegate_xwayland_shell;
use smithay::desktop::Window;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::selection::SelectionTarget;
use smithay::wayland::selection::data_device::{
    clear_data_device_selection, request_data_device_client_selection, set_data_device_selection,
};
use smithay::wayland::selection::primary_selection::{
    clear_primary_selection, request_primary_client_selection, set_primary_selection,
};
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::xwm::{Reorder, ResizeEdge, XwmId};
use smithay::xwayland::{X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler};

use super::super::grabs;
use super::super::shell;
use super::super::state::Slate;

pub fn start(state: &mut Slate) {
    let spawned = XWayland::spawn(
        &state.display_handle,
        None,
        std::iter::empty::<(String, String)>(),
        true,
        Stdio::null(),
        Stdio::null(),
        |_| {},
    );
    let (xwayland, client) = match spawned {
        Ok(spawned) => spawned,
        Err(error) => {
            tracing::error!(%error, "failed to start XWayland");
            return;
        }
    };

    let loop_handle = state.loop_handle.clone();
    let inserted = state.loop_handle.insert_source(xwayland, move |event, _, state| match event {
        XWaylandEvent::Ready {
            x11_socket,
            display_number,
        } => match X11Wm::start_wm(loop_handle.clone(), x11_socket, client.clone()) {
            Ok(xwm) => {
                tracing::info!(display = display_number, "XWayland is ready");
                state.xwm = Some(xwm);
                state.xdisplay = Some(display_number);
            }
            Err(error) => tracing::error!(%error, "failed to start the X11 window manager"),
        },
        XWaylandEvent::Error => tracing::error!("XWayland exited during startup"),
    });
    if let Err(error) = inserted {
        tracing::error!(%error, "failed to watch XWayland");
    }
}

impl XWaylandShellHandler for Slate {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell_state
    }
}

impl XwmHandler for Slate {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm.as_mut().unwrap()
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Err(error) = surface.set_mapped(true) {
            tracing::warn!(%error, "failed to map X11 window");
            return;
        }
        shell::map_window(self, Window::new_x11_window(surface));
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        let location = surface.geometry().loc;
        self.space.map_element(Window::new_x11_window(surface), location, true);
    }

    fn unmapped_window(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(window) = shell::window_for_x11(self, &surface) {
            shell::unmap_window(self, &window);
        }
        if !surface.is_override_redirect() {
            let _ = surface.set_mapped(false);
        }
    }

    fn destroyed_window(&mut self, _xwm: XwmId, _surface: X11Surface) {}

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        surface: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        let mut geometry = surface.geometry();
        if let Some(window) = shell::window_for_x11(self, &surface) {
            if let Some(location) = self.space.element_location(&window) {
                geometry.loc = location;
            }
        } else {
            if let Some(x) = x {
                geometry.loc.x = x;
            }
            if let Some(y) = y {
                geometry.loc.y = y;
            }
        }
        if let Some(w) = w {
            geometry.size.w = w as i32;
        }
        if let Some(h) = h {
            geometry.size.h = h as i32;
        }
        let _ = surface.configure(geometry);
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        surface: X11Surface,
        geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
        if !surface.is_override_redirect() {
            return;
        }
        if let Some(window) = shell::window_for_x11(self, &surface) {
            self.space.map_element(window, geometry.loc, false);
        }
    }

    fn maximize_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(window) = shell::window_for_x11(self, &surface) {
            shell::set_maximized(self, &window, true);
        }
    }

    fn unmaximize_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(window) = shell::window_for_x11(self, &surface) {
            shell::set_maximized(self, &window, false);
        }
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(window) = shell::window_for_x11(self, &surface) {
            shell::set_fullscreen(self, &window, true);
        }
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, surface: X11Surface) {
        if let Some(window) = shell::window_for_x11(self, &surface) {
            shell::set_fullscreen(self, &window, false);
        }
    }

    fn resize_request(&mut self, _xwm: XwmId, surface: X11Surface, _button: u32, edge: ResizeEdge) {
        let Some(window) = shell::window_for_x11(self, &surface) else {
            return;
        };
        let seat = self.seat.clone();
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        grabs::start_resize(self, &seat, window, edge.into(), serial, None);
    }

    fn move_request(&mut self, _xwm: XwmId, surface: X11Surface, _button: u32) {
        let Some(window) = shell::window_for_x11(self, &surface) else {
            return;
        };
        let seat = self.seat.clone();
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        grabs::start_move(self, &seat, window, serial, None);
    }

    fn send_selection(&mut self, _xwm: XwmId, target: SelectionTarget, mime_type: String, fd: OwnedFd) {
        let requested = match target {
            SelectionTarget::Clipboard => request_data_device_client_selection(&self.seat, mime_type, fd).map_err(|error| error.to_string()),
            SelectionTarget::Primary => request_primary_client_selection(&self.seat, mime_type, fd).map_err(|error| error.to_string()),
        };
        if let Err(error) = requested {
            tracing::warn!(%error, "failed to request Wayland selection for XWayland");
        }
    }

    fn new_selection(&mut self, _xwm: XwmId, target: SelectionTarget, mime_types: Vec<String>) {
        match target {
            SelectionTarget::Clipboard => set_data_device_selection(&self.display_handle, &self.seat, mime_types, ()),
            SelectionTarget::Primary => set_primary_selection(&self.display_handle, &self.seat, mime_types, ()),
        }
    }

    fn cleared_selection(&mut self, _xwm: XwmId, target: SelectionTarget) {
        match target {
            SelectionTarget::Clipboard => clear_data_device_selection(&self.display_handle, &self.seat),
            SelectionTarget::Primary => clear_primary_selection(&self.display_handle, &self.seat),
        }
    }

    fn disconnected(&mut self, _xwm: XwmId) {
        self.xwm = None;
    }
}

delegate_xwayland_shell!(Slate);
