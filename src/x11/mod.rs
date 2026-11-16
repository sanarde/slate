// SPDX-License-Identifier: GPL-3.0-only

mod atoms;
mod framing;
mod geometry;
mod keys;
mod pixels;
mod wm;

use std::error::Error;

use x11rb::rust_connection::RustConnection;

use crate::config::Settings;
use crate::source::ConfigSource;
use crate::spawn;

pub fn run(command: &[String], settings: Settings, config: ConfigSource) -> Result<(), Box<dyn Error>> {
    let (conn, screen_num) = RustConnection::connect(None)?;
    let mut wm = wm::Wm::new(conn, screen_num, settings, config)?;

    tracing::info!("managing the X11 display");
    let envs = wm.environment();
    spawn::spawn(command, &envs);
    crate::session::start(wm.settings(), &envs, true);

    wm.run()
}
