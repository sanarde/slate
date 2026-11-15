// SPDX-License-Identifier: GPL-3.0-only

mod wayland;

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();
    wayland::run()
}
