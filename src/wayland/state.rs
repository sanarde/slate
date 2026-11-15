// SPDX-License-Identifier: GPL-3.0-only

use std::error::Error;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Instant;

use smithay::desktop::{Space, Window};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_server::backend::ClientData;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::socket::ListeningSocketSource;

pub struct Slate {
    pub start_time: Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,
    pub space: Space<Window>,
    pub compositor_state: CompositorState,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {}

impl Slate {
    pub fn new(event_loop: &EventLoop<Slate>, display: Display<Slate>) -> Result<Slate, Box<dyn Error>> {
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

        let compositor_state = CompositorState::new::<Slate>(&display_handle);

        Ok(Slate {
            start_time: Instant::now(),
            socket_name,
            display_handle,
            loop_signal: event_loop.get_signal(),
            space: Space::default(),
            compositor_state,
        })
    }
}
