// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::{self, LIST_KEYS};
use crate::themes;

const MAX_DEPTH: usize = 8;

#[derive(Default)]
pub struct Layer {
    pub vars: HashMap<String, String>,
    pub scalars: Vec<(String, String)>,
    pub lists: HashMap<String, Vec<String>>,
    pub warnings: Vec<String>,
    pub files: Vec<PathBuf>,
}

impl Layer {
    pub fn scalar(&self, key: &str) -> Option<&str> {
        self.scalars
            .iter()
            .rev()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value.as_str())
    }
}

fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'#' {
            continue;
        }
        let after_space = index > 0 && bytes[index - 1].is_ascii_whitespace();
        let before_space = bytes.get(index + 1).is_none_or(|next| next.is_ascii_whitespace());
        if (index == 0 || after_space) && before_space {
            return &line[..index];
        }
    }
    line
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(quote).and_then(|rest| rest.strip_suffix(quote)) {
            return inner;
        }
    }
    value
}

fn expand(path: &str, base: Option<&Path>) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return themes::home().join(rest);
    }
    let path = Path::new(path);
    match base {
        Some(base) if path.is_relative() => base.join(path),
        _ => path.to_path_buf(),
    }
}

pub fn read_path(layer: &mut Layer, path: &Path, depth: usize) {
    layer.files.push(path.to_path_buf());
    match std::fs::read_to_string(path) {
        Ok(text) => read_text(layer, &text, &path.display().to_string(), path.parent(), depth),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && depth == 0 => {}
        Err(error) => layer.warnings.push(format!("{}: {error}", path.display())),
    }
}

fn include(layer: &mut Layer, target: &str, origin: &str, base: Option<&Path>, depth: usize) {
    if depth >= MAX_DEPTH {
        layer.warnings.push(format!("{origin}: includes are nested too deeply"));
        return;
    }

    if let Some(name) = target.strip_prefix("theme:") {
        match themes::find(name) {
            Some((text, path)) => {
                if let Some(path) = &path {
                    layer.files.push(path.clone());
                }
                read_text(layer, &text, &format!("theme {name}"), path.as_deref().and_then(Path::parent), depth + 1);
            }
            None => layer.warnings.push(format!("{origin}: theme {name} was not found")),
        }
        return;
    }
    read_path(layer, &expand(target, base), depth + 1);
}

pub fn read_text(layer: &mut Layer, text: &str, origin: &str, base: Option<&Path>, depth: usize) {
    for (index, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() || (line.starts_with('[') && line.ends_with(']')) {
            continue;
        }
        let location = format!("{origin}:{}", index + 1);

        let Some((key, value)) = line.split_once('=') else {
            layer.warnings.push(format!("{location}: expected key = value"));
            continue;
        };
        let key = key.trim();
        let value = unquote(value.trim());

        if key == "include" {
            include(layer, value, &location, base, depth);
        } else if let Some(name) = key.strip_prefix('$') {
            layer.vars.insert(name.to_string(), value.to_string());
        } else if key == "bind" || key == "rule" || key == "float" {
            let list = match key {
                "bind" => "bindings",
                "rule" => "decoration-rules",
                _ => "floating-rules",
            };
            layer.lists.entry(list.to_string()).or_default().push(value.to_string());
        } else if LIST_KEYS.contains(&key) {
            let entries = value.split(';').map(str::trim).filter(|entry| !entry.is_empty());
            layer.lists.entry(key.to_string()).or_default().extend(entries.map(String::from));
        } else if config::is_known(key) {
            layer.scalars.push((key.to_string(), value.to_string()));
        } else {
            layer.warnings.push(format!("{location}: unknown key {key}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Layer {
        let mut layer = Layer::default();
        read_text(&mut layer, text, "test", None, 0);
        layer
    }

    #[test]
    fn keys_variables_lists_and_comments() {
        let layer = read(
            "# comment\n[/org/nuros/sanar/slate]\n$accent = #6750a4  # trailing\ntitlebar-height = 28\nborder-color-focused = $accent\n\
             bind = Mod+b=exec firefox\nbind = Mod+q=none\nrule = foot=none\nbindings = Mod+x=quit;Mod+y=close\n",
        );

        assert!(layer.warnings.is_empty(), "{:?}", layer.warnings);
        assert_eq!(layer.vars.get("accent").map(String::as_str), Some("#6750a4"));
        assert_eq!(layer.scalar("titlebar-height"), Some("28"));
        assert_eq!(layer.scalar("border-color-focused"), Some("$accent"));
        assert_eq!(layer.lists["bindings"].len(), 4);
        assert_eq!(layer.lists["decoration-rules"], vec!["foot=none".to_string()]);
    }

    #[test]
    fn colors_are_not_comments_and_unknown_keys_warn() {
        let layer = read("button-color = #ffffff33\ntitle = \"quoted\"\nno equals sign\n");

        assert_eq!(layer.scalar("button-color"), Some("#ffffff33"));
        assert_eq!(layer.warnings.len(), 2);
        assert!(layer.warnings[0].contains("unknown key title"));
        assert!(layer.warnings[1].contains("expected key = value"));
    }

    #[test]
    fn a_later_value_wins_and_themes_can_be_included() {
        let layer = read("titlebar-height = 20\ninclude = theme:minimal\ntitlebar-height = 36\n");

        assert_eq!(layer.scalar("titlebar-height"), Some("36"));
        assert_eq!(layer.scalar("border-width"), Some("2"));
        assert!(layer.warnings.is_empty(), "{:?}", layer.warnings);
    }
}
