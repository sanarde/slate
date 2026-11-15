// SPDX-License-Identifier: GPL-3.0-only

mod handlers;
mod state;
mod winit;

use std::error::Error;

use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;

use state::Slate;

pub fn run() -> Result<(), Box<dyn Error>> {
    let mut event_loop: EventLoop<Slate> = EventLoop::try_new()?;
    let display: Display<Slate> = Display::new()?;
    let mut state = Slate::new(&event_loop, display)?;

    winit::init(&event_loop, &mut state)?;

    tracing::info!(socket = ?state.socket_name, "listening on Wayland socket");

    event_loop.run(None, &mut state, dispatch_idle)?;
    Ok(())
}

fn dispatch_idle(state: &mut Slate) {
    state.space.refresh();
    if let Err(error) = state.display_handle.flush_clients() {
        tracing::error!(%error, "failed to flush clients");
    }
}
