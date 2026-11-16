// SPDX-License-Identifier: GPL-3.0-only

mod atoms;
mod framing;
mod geometry;
mod keys;
mod pixels;
mod wm;
mod xcb;

use std::error::Error;

use x11rb::xcb_ffi::XCBConnection;

use crate::config::Settings;
use crate::source::ConfigSource;
use crate::spawn;

pub fn run(command: &[String], settings: Settings, config: ConfigSource) -> Result<(), Box<dyn Error>> {
    let (conn, screen_num) = XCBConnection::connect(None)?;
    let mut wm = wm::Wm::new(conn, screen_num, settings, config)?;

    tracing::info!("managing the X11 display");
    let envs = wm.environment();
    spawn::spawn(command, &envs);
    crate::session::start(wm.settings(), &envs, true);

    wm.run()
}
