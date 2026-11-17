// SPDX-License-Identifier: GPL-3.0-only

pub mod decoration;
pub mod dmabuf;
pub mod layer;
pub mod protocols;
pub mod xdg;
pub mod xwayland;

use std::os::fd::OwnedFd;

use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::Client;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{CompositorClientState, CompositorHandler, CompositorState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler, set_data_device_focus,
};
use smithay::wayland::selection::primary_selection::set_primary_focus;
use smithay::wayland::selection::{SelectionHandler, SelectionSource, SelectionTarget};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::xwayland::XWaylandClientData;
use smithay::{delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm};

use super::focus::KeyFocus;
use super::shell;
use super::state::{ClientState, Slate};

impl CompositorHandler for Slate {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        if let Some(state) = client.get_data::<XWaylandClientData>() {
            return &state.compositor_state;
        }
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        layer::guard_destroyed_role(surface);
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Slate>(surface);
        shell::handle_commit(self, surface);
    }
}

impl BufferHandler for Slate {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for Slate {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl SeatHandler for Slate {
    type KeyboardFocus = KeyFocus;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Slate> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Slate>, focused: Option<&KeyFocus>) {
        let client = focused
            .and_then(|focus| focus.wl_surface())
            .and_then(|surface| surface.client());
        set_primary_focus(&self.display_handle, seat, client.clone());
        set_data_device_focus(&self.display_handle, seat, client);
    }

    fn cursor_image(&mut self, _seat: &Seat<Slate>, image: CursorImageStatus) {
        self.cursor_status = image;
    }
}

impl SelectionHandler for Slate {
    type SelectionUserData = ();

    fn new_selection(&mut self, target: SelectionTarget, source: Option<SelectionSource>, _seat: Seat<Slate>) {
        let Some(xwm) = self.xwm.as_mut() else {
            return;
        };
        if let Err(error) = xwm.new_selection(target, source.map(|source| source.mime_types())) {
            tracing::warn!(%error, "failed to pass selection to XWayland");
        }
    }

    fn send_selection(
        &mut self,
        target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Slate>,
        _user_data: &(),
    ) {
        let Some(xwm) = self.xwm.as_mut() else {
            return;
        };
        if let Err(error) = xwm.send_selection(target, mime_type, fd, self.loop_handle.clone()) {
            tracing::warn!(%error, "failed to send selection to XWayland");
        }
    }
}

impl DataDeviceHandler for Slate {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for Slate {}

impl ServerDndGrabHandler for Slate {}

impl OutputHandler for Slate {}

delegate_compositor!(Slate);
delegate_shm!(Slate);
delegate_seat!(Slate);
delegate_data_device!(Slate);
delegate_output!(Slate);
