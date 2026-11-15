// SPDX-License-Identifier: GPL-3.0-only

mod spawn;
mod wayland;
mod x11;

use std::error::Error;

const USAGE: &str = "usage: slate [--backend auto|winit|udev|x11] [COMMAND [ARGUMENT...]]";

#[derive(Clone, Copy, PartialEq)]
pub enum Backend {
    Winit,
    Udev,
    X11,
}

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().init();

    let mut args = std::env::args().skip(1).peekable();
    let mut backend = None;

    while let Some(arg) = args.peek() {
        match arg.as_str() {
            "--backend" => {
                args.next();
                backend = match args.next().as_deref() {
                    Some("auto") => None,
                    Some("winit") => Some(Backend::Winit),
                    Some("udev") => Some(Backend::Udev),
                    Some("x11") => Some(Backend::X11),
                    _ => return Err(USAGE.into()),
                };
            }
            "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--version" => {
                println!("slate {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            option if option.starts_with('-') => return Err(USAGE.into()),
            _ => break,
        }
    }

    let command: Vec<String> = args.collect();

    match backend.unwrap_or_else(detect_backend) {
        Backend::X11 => x11::run(&command),
        backend => wayland::run(backend, &command),
    }
}

fn detect_backend() -> Backend {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some() {
        Backend::Winit
    } else {
        Backend::Udev
    }
}
