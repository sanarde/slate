// SPDX-License-Identifier: GPL-3.0-only

mod animation;
mod cursor;
mod decoration;
mod focus;
mod foreign_toplevel;
mod grabs;
mod handlers;
mod input;
mod ipc;
mod lock;
mod output_management;
mod outputs;
mod render;
mod scale;
mod screencopy;
mod settings;
mod shadow;
mod shell;
mod state;
mod tiling;
mod udev;
mod winit;

use std::error::Error;

use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;

use crate::Backend;
use crate::config::Settings;
use crate::source::ConfigSource;
use state::{ModKey, Slate};

pub fn run(
    backend: Backend,
    command: &[String],
    settings: Settings,
    config: ConfigSource,
) -> Result<(), Box<dyn Error>> {
    let mut event_loop: EventLoop<'static, Slate> = EventLoop::try_new()?;
    let display: Display<Slate> = Display::new()?;

    let default_mod_key = if backend == Backend::Udev {
        ModKey::Logo
    } else {
        ModKey::Alt
    };
    let mut state = Slate::new(&event_loop, display, settings, config, default_mod_key)?;
    settings::watch(&event_loop, &mut state)?;
    ipc::start(&event_loop, &mut state)?;

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
    lock::prune(state);
    if let Err(error) = state.display_handle.flush_clients() {
        tracing::error!(%error, "failed to flush clients");
    }
}
