// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::OsString;
use std::process::Command;

pub fn terminal_command(configured: Option<&str>, default: &str) -> Vec<String> {
    let environment = std::env::var("TERMINAL").ok().filter(|command| !command.is_empty());
    let command = configured.or(environment.as_deref()).unwrap_or(default);
    command.split_whitespace().map(String::from).collect()
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
