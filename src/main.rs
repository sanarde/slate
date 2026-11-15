// SPDX-License-Identifier: GPL-3.0-only

mod bindings;
mod cfgd;
mod conf;
mod config;
mod layout;
mod scheme;
mod source;
mod spawn;
mod themes;
mod wayland;
mod x11;

use std::error::Error;

use source::{ConfigSource, Options};

const USAGE: &str = "usage: slate [OPTION...] [COMMAND [ARGUMENT...]]

options:
  --backend auto|winit|udev|x11   display backend, auto picks winit inside a session and udev otherwise
  --config PATH                   configuration file, default $XDG_CONFIG_HOME/slate/slate.conf
  --theme NAME                    use this theme instead of the configured one
  --dump-config                   print the effective settings and where each value comes from
  --check                         read the configuration, print problems and exit
  --list-themes                   print the available themes
  --help, --version";

#[derive(Clone, Copy, PartialEq)]
pub enum Backend {
    Winit,
    Udev,
    X11,
}

enum Mode {
    Run,
    DumpConfig,
    Check,
    ListThemes,
}

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();

    let mut args = std::env::args().skip(1).peekable();
    let mut backend = None;
    let mut options = Options::default();
    let mut mode = Mode::Run;

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
            "--config" => {
                args.next();
                options.config_path = Some(args.next().ok_or(USAGE)?.into());
            }
            "--theme" => {
                args.next();
                options.theme = Some(args.next().ok_or(USAGE)?);
            }
            "--dump-config" => {
                args.next();
                mode = Mode::DumpConfig;
            }
            "--check" => {
                args.next();
                mode = Mode::Check;
            }
            "--list-themes" => {
                args.next();
                mode = Mode::ListThemes;
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

    match mode {
        Mode::ListThemes => {
            for (name, origin) in themes::list() {
                println!("{name}\t{origin}");
            }
            return Ok(());
        }
        Mode::DumpConfig => {
            print!("{}", source::dump(&ConfigSource::new(options).load()));
            return Ok(());
        }
        Mode::Check => {
            let resolved = ConfigSource::new(options).load();
            for warning in &resolved.warnings {
                eprintln!("{warning}");
            }
            if resolved.warnings.is_empty() {
                println!("{}: no problems found", resolved.config_path.display());
                return Ok(());
            }
            std::process::exit(1);
        }
        Mode::Run => {}
    }

    let command: Vec<String> = args.collect();

    let mut config = ConfigSource::new(options);
    let settings = config.settings();

    match backend.unwrap_or_else(detect_backend) {
        Backend::X11 => x11::run(&command, settings, config),
        backend => wayland::run(backend, &command, settings, config),
    }
}

fn detect_backend() -> Backend {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some() {
        Backend::Winit
    } else {
        Backend::Udev
    }
}
