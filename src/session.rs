// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::bindings::split_command;
use crate::config::Settings;
use crate::spawn;

const DESKTOP_NAME: &str = "Slate";
const IMPORTED: [&str; 7] = [
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "SLATE_SOCKET",
    "XCURSOR_THEME",
    "XCURSOR_SIZE",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
];

pub fn desktops() -> Vec<String> {
    let mut names: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|name| !name.is_empty())
        .map(String::from)
        .collect();
    if !names.iter().any(|name| name == DESKTOP_NAME) {
        names.push(DESKTOP_NAME.to_string());
    }
    names
}

pub fn parse_desktop_entry(text: &str, desktops: &[String]) -> Option<Vec<String>> {
    let mut in_entry = false;
    let mut exec = None;
    let mut try_exec = None;
    let mut only_show_in: Vec<&str> = Vec::new();
    let mut not_show_in: Vec<&str> = Vec::new();

    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_entry) else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Exec" => exec = Some(value),
            "TryExec" => try_exec = Some(value),
            "Hidden" if value == "true" => return None,
            "X-GNOME-Autostart-enabled" if value == "false" => return None,
            "OnlyShowIn" => only_show_in = value.split(';').filter(|name| !name.is_empty()).collect(),
            "NotShowIn" => not_show_in = value.split(';').filter(|name| !name.is_empty()).collect(),
            _ => {}
        }
    }

    let listed = |names: &[&str]| names.iter().any(|name| desktops.iter().any(|desktop| desktop == name));
    if (!only_show_in.is_empty() && !listed(&only_show_in)) || listed(&not_show_in) {
        return None;
    }
    if let Some(program) = try_exec
        && !program_exists(program)
    {
        return None;
    }

    let command: Vec<String> = split_command(exec?)
        .ok()?
        .into_iter()
        .filter(|word| !(word.len() == 2 && word.starts_with('%')))
        .collect();
    (!command.is_empty()).then_some(command)
}

fn program_exists(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).exists();
    }
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).exists()))
}

fn autostart_directories() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    let system = std::env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".to_string());

    home.into_iter()
        .chain(system.split(':').filter(|dir| !dir.is_empty()).map(PathBuf::from))
        .map(|dir| dir.join("autostart"))
        .collect()
}

pub fn autostart_commands(directories: &[PathBuf], desktops: &[String]) -> Vec<Vec<String>> {
    let mut seen = HashSet::new();
    let mut commands = Vec::new();

    for directory in directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "desktop"))
            .collect();
        files.sort();

        for path in files {
            let Some(name) = path.file_name().map(|name| name.to_os_string()) else {
                continue;
            };
            if !seen.insert(name) {
                continue;
            }
            if let Some(command) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| parse_desktop_entry(&text, desktops))
            {
                commands.push(command);
            }
        }
    }
    commands
}

pub fn start(settings: &Settings, envs: &[(&'static str, OsString)], import_environment: bool) {
    if std::env::var_os("SLATE_RESTARTED").is_some() {
        unsafe { std::env::remove_var("SLATE_RESTARTED") };
        return;
    }

    let mut envs: Vec<(&'static str, OsString)> = envs.to_vec();
    if !envs.iter().any(|(name, _)| *name == "XDG_CURRENT_DESKTOP") {
        envs.push(("XDG_CURRENT_DESKTOP", OsString::from(desktops().join(":"))));
    }

    if import_environment && settings.session_environment {
        let mut command = vec!["dbus-update-activation-environment".to_string(), "--systemd".to_string()];
        command.extend(
            IMPORTED
                .iter()
                .filter(|name| envs.iter().any(|(candidate, _)| candidate == *name))
                .map(|name| name.to_string()),
        );
        spawn::spawn(&command, &envs);
    }

    for command in &settings.exec_once {
        spawn::spawn(command, &envs);
    }
    if settings.xdg_autostart {
        for command in autostart_commands(&autostart_directories(), &desktops()) {
            spawn::spawn(&command, &envs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn desktop_entries_become_commands() {
        let text = "[Desktop Entry]\nName=Applet\nExec=nm-applet --indicator %U\n[Desktop Action x]\nExec=other\n";
        assert_eq!(
            parse_desktop_entry(text, &names(&["Slate"])),
            Some(vec!["nm-applet".to_string(), "--indicator".to_string()])
        );
    }

    #[test]
    fn hidden_and_foreign_entries_are_skipped() {
        let slate = names(&["Slate"]);
        assert_eq!(parse_desktop_entry("[Desktop Entry]\nExec=a\nHidden=true\n", &slate), None);
        assert_eq!(parse_desktop_entry("[Desktop Entry]\nExec=a\nOnlyShowIn=GNOME;\n", &slate), None);
        assert_eq!(parse_desktop_entry("[Desktop Entry]\nExec=a\nNotShowIn=Slate;\n", &slate), None);
        assert_eq!(parse_desktop_entry("[Desktop Entry]\nExec=a\nTryExec=/nonexistent/program\n", &slate), None);
        assert!(parse_desktop_entry("[Desktop Entry]\nExec=a\nOnlyShowIn=Slate;\n", &slate).is_some());
    }

    #[test]
    fn the_first_directory_wins_by_file_name() {
        let root = std::env::temp_dir().join(format!("slate-autostart-{}", std::process::id()));
        let (first, second) = (root.join("first"), root.join("second"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("a.desktop"), "[Desktop Entry]\nExec=mine\n").unwrap();
        std::fs::write(second.join("a.desktop"), "[Desktop Entry]\nExec=system\n").unwrap();
        std::fs::write(second.join("b.desktop"), "[Desktop Entry]\nExec=other\n").unwrap();

        let commands = autostart_commands(&[first, second], &names(&["Slate"]));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(commands, vec![vec!["mine".to_string()], vec!["other".to_string()]]);
    }
}
