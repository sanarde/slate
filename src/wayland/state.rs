// SPDX-License-Identifier: GPL-3.0-only

use std::error::Error;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Instant;

use smithay::desktop::{PopupManager, Space, Window, layer_map_for_output};
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_server::backend::ClientData;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::xwayland_shell::XWaylandShellState;
use smithay::xwayland::X11Wm;

use super::cursor::Cursors;
use super::grabs::Resizing;
use super::udev::Udev;
use crate::spawn;

pub const WORKSPACE_COUNT: usize = 9;
const KEY_REPEAT_DELAY_MS: i32 = 200;
const KEY_REPEAT_RATE_HZ: i32 = 25;

#[derive(Clone, Copy)]
pub enum ModKey {
    Alt,
    Logo,
}

pub struct Slate {
    pub start_time: Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, Slate>,
    pub loop_signal: LoopSignal,
    pub space: Space<Window>,
    pub pending_windows: Vec<Window>,
    pub parked: Vec<(Window, Point<i32, Logical>)>,
    pub workspace: usize,
    pub resizing: Option<Resizing>,
    pub popups: PopupManager,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub xwayland_shell_state: XWaylandShellState,
    pub output_manager_state: OutputManagerState,
    pub shm_state: ShmState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub seat_state: SeatState<Slate>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<Slate>,
    pub cursor_status: CursorImageStatus,
    pub cursors: Cursors,
    pub mod_key: ModKey,
    pub xwm: Option<X11Wm>,
    pub xdisplay: Option<u32>,
    pub udev: Option<Udev>,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {}

impl Slate {
    pub fn new(
        event_loop: &EventLoop<'static, Slate>,
        display: Display<Slate>,
        mod_key: ModKey,
    ) -> Result<Slate, Box<dyn Error>> {
        let display_handle = display.handle();
        let loop_handle = event_loop.handle();

        let socket = ListeningSocketSource::new_auto()?;
        let socket_name = socket.socket_name().to_os_string();

        loop_handle.insert_source(socket, |stream, _, state| {
            if let Err(error) = state.display_handle.insert_client(stream, Arc::new(ClientState::default())) {
                tracing::error!(%error, "failed to add client");
            }
        })?;

        loop_handle.insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state| {
                unsafe { display.get_mut().dispatch_clients(state)? };
                Ok(PostAction::Continue)
            },
        )?;

        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&display_handle, "seat0");
        seat.add_keyboard(Default::default(), KEY_REPEAT_DELAY_MS, KEY_REPEAT_RATE_HZ)?;
        seat.add_pointer();

        Ok(Slate {
            start_time: Instant::now(),
            socket_name,
            loop_handle,
            loop_signal: event_loop.get_signal(),
            space: Space::default(),
            pending_windows: Vec::new(),
            parked: Vec::new(),
            workspace: 0,
            resizing: None,
            popups: PopupManager::default(),
            compositor_state: CompositorState::new::<Slate>(&display_handle),
            xdg_shell_state: XdgShellState::new::<Slate>(&display_handle),
            layer_shell_state: WlrLayerShellState::new::<Slate>(&display_handle),
            xwayland_shell_state: XWaylandShellState::new::<Slate>(&display_handle),
            output_manager_state: OutputManagerState::new_with_xdg_output::<Slate>(&display_handle),
            shm_state: ShmState::new::<Slate>(&display_handle, vec![]),
            dmabuf_state: DmabufState::new(),
            dmabuf_global: None,
            data_device_state: DataDeviceState::new::<Slate>(&display_handle),
            seat_state,
            seat,
            cursor_status: CursorImageStatus::default_named(),
            cursors: Cursors::new(),
            mod_key,
            xwm: None,
            xdisplay: None,
            udev: None,
            display_handle,
        })
    }

    pub fn spawn(&self, command: &[String]) {
        let mut envs = vec![("WAYLAND_DISPLAY", self.socket_name.clone())];
        if let Some(number) = self.xdisplay {
            envs.push(("DISPLAY", OsString::from(format!(":{number}"))));
        }
        spawn::spawn(command, &envs);
    }

    pub fn send_frames(&self, output: &Output) {
        let elapsed = self.start_time.elapsed();

        for window in self.space.elements() {
            window.send_frame(output, elapsed, None, |_, _| Some(output.clone()));
        }

        let layers = layer_map_for_output(output);
        for layer in layers.layers() {
            layer.send_frame(output, elapsed, None, |_, _| Some(output.clone()));
        }
    }
}
