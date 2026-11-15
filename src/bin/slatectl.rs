// SPDX-License-Identifier: GPL-3.0-only

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: slatectl COMMAND [ARGUMENT...]

commands:
  state | windows | workspaces | outputs      print the state as JSON
  version                                     print the version
  subscribe                                   print events as JSON lines until interrupted
  action ACTION [ARGUMENT...]                 run an action of the key bindings, for example: action workspace 2
  focus ID | close ID | minimize ID | restore ID
  move-to-workspace ID N
  layout N NAME                               set the layout of workspace N

The socket is $SLATE_SOCKET, or $XDG_RUNTIME_DIR/slate.sock.";

fn socket() -> PathBuf {
    if let Some(path) = std::env::var_os("SLATE_SOCKET").filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    runtime.join("slate.sock")
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.is_empty() || arguments[0] == "--help" || arguments[0] == "-h" {
        println!("{USAGE}");
        return if arguments.is_empty() { ExitCode::FAILURE } else { ExitCode::SUCCESS };
    }

    let path = socket();
    let stream = match UnixStream::connect(&path) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("slatectl: cannot connect to {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };

    let mut writer = &stream;
    if writeln!(writer, "{}", arguments.join(" ")).is_err() {
        eprintln!("slatectl: the connection was closed");
        return ExitCode::FAILURE;
    }

    let subscribing = arguments[0] == "subscribe";
    let mut failed = false;
    for line in BufReader::new(&stream).lines() {
        let Ok(line) = line else {
            break;
        };
        println!("{line}");
        if !subscribing {
            failed = line.contains("\"ok\":false");
            break;
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
