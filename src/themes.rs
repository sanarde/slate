// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

pub const BUILTIN: &[(&str, &str)] = &[
    ("material-you", include_str!("../data/themes/material-you.conf")),
    ("minimal", include_str!("../data/themes/minimal.conf")),
    ("nord", include_str!("../data/themes/nord.conf")),
    ("gruvbox-dark", include_str!("../data/themes/gruvbox-dark.conf")),
    ("catppuccin-mocha", include_str!("../data/themes/catppuccin-mocha.conf")),
    ("dracula", include_str!("../data/themes/dracula.conf")),
    ("tokyo-night", include_str!("../data/themes/tokyo-night.conf")),
    ("light", include_str!("../data/themes/light.conf")),
];

pub fn config_home() -> PathBuf {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => home().join(".config"),
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

pub fn directories() -> Vec<PathBuf> {
    let mut directories = vec![config_home().join("slate/themes")];

    match std::env::var_os("XDG_DATA_HOME") {
        Some(path) if !path.is_empty() => directories.push(PathBuf::from(path).join("slate/themes")),
        _ => directories.push(home().join(".local/share/slate/themes")),
    }

    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for directory in data_dirs.split(':').filter(|directory| !directory.is_empty()) {
        directories.push(PathBuf::from(directory).join("slate/themes"));
    }
    directories
}

fn valid(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|character| character.is_alphanumeric() || character == '-' || character == '_')
}

pub fn find(name: &str) -> Option<(String, Option<PathBuf>)> {
    if !valid(name) {
        return None;
    }

    for directory in directories() {
        let path = directory.join(format!("{name}.conf"));
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Some((text, Some(path)));
        }
    }
    BUILTIN
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, text)| (text.to_string(), None))
}

pub fn list() -> Vec<(String, String)> {
    let mut themes: Vec<(String, String)> = Vec::new();

    for directory in directories() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension()? == "conf").then(|| path.file_stem()?.to_str().map(String::from))?
            })
            .collect();
        names.sort();
        for name in names {
            if !themes.iter().any(|(known, _)| *known == name) {
                themes.push((name, directory.display().to_string()));
            }
        }
    }
    for (name, _) in BUILTIN {
        if !themes.iter().any(|(known, _)| known == name) {
            themes.push((name.to_string(), "built in".to_string()));
        }
    }
    themes
}
