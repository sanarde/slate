// SPDX-License-Identifier: GPL-3.0-only

mod cursor;
mod focus;
mod grabs;
mod handlers;
mod input;
mod output_management;
mod outputs;
mod render;
mod settings;
mod shell;
mod state;
mod udev;
mod winit;

use std::error::Error;

use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;

use crate::Backend;
use crate::cfgd::Cfgd;
use crate::config::Settings;
use state::{ModKey, Slate};

pub fn run(
    backend: Backend,
    command: &[String],
    settings: Settings,
    cfgd: Option<Cfgd>,
) -> Result<(), Box<dyn Error>> {
    let mut event_loop: EventLoop<'static, Slate> = EventLoop::try_new()?;
    let display: Display<Slate> = Display::new()?;

    let default_mod_key = if backend == Backend::Udev {
        ModKey::Logo
    } else {
        ModKey::Alt
    };
    let mut state = Slate::new(&event_loop, display, settings, cfgd, default_mod_key)?;
    settings::watch(&event_loop, &mut state)?;

    if backend == Backend::Udev {
        udev::init(&event_loop, &mut state)?;
    } else {
        winit::init(&event_loop, &mut state)?;
    }

    handlers::xwayland::start(&mut state);

    tracing::info!(socket = ?state.socket_name, "listening on Wayland socket");
    state.spawn(command);

    event_loop.run(None, &mut state, dispatch_idle)?;
    Ok(())
}

fn dispatch_idle(state: &mut Slate) {
    state.space.refresh();
    state.popups.cleanup();
    if let Err(error) = state.display_handle.flush_clients() {
        tracing::error!(%error, "failed to flush clients");
    }
}
