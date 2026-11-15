// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::OsString;
use std::process::Command;

pub fn terminal_command(default: &str) -> Vec<String> {
    vec![std::env::var("TERMINAL").unwrap_or_else(|_| default.to_string())]
}

pub fn spawn(command: &[String], envs: &[(&str, OsString)]) {
    let Some((program, arguments)) = command.split_first() else {
        return;
    };

    let mut process = Command::new(program);
    process.args(arguments);
    for (name, value) in envs {
        process.env(name, value);
    }

    match process.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(error) => tracing::error!(%error, program, "failed to start program"),
    }
}
