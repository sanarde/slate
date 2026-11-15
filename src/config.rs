// SPDX-License-Identifier: GPL-3.0-only

use crate::cfgd::Cfgd;

pub const PREFIX: &str = "/org/nuros/sanar/slate/";

#[derive(Clone, Copy, PartialEq)]
pub enum ModKeySetting {
    Auto,
    Super,
    Alt,
}

#[derive(Clone, Default, PartialEq)]
pub struct Xkb {
    pub rules: String,
    pub model: String,
    pub layout: String,
    pub variant: String,
    pub options: Option<String>,
}

#[derive(Clone, PartialEq)]
pub struct Settings {
    pub mod_key: ModKeySetting,
    pub terminal: Option<String>,
    pub workspace_count: usize,
    pub border_width: u32,
    pub border_focused: u32,
    pub border_unfocused: u32,
    pub xkb: Xkb,
    pub repeat_delay: i32,
    pub repeat_rate: i32,
    pub cursor_theme: Option<String>,
    pub cursor_size: u32,
    pub tap_to_click: bool,
    pub natural_scroll: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            mod_key: ModKeySetting::Auto,
            terminal: None,
            workspace_count: 9,
            border_width: 2,
            border_focused: 0x6750a4,
            border_unfocused: 0x49454f,
            xkb: Xkb::default(),
            repeat_delay: 200,
            repeat_rate: 25,
            cursor_theme: None,
            cursor_size: 0,
            tap_to_click: true,
            natural_scroll: false,
        }
    }
}

fn key(name: &str) -> String {
    format!("{PREFIX}{name}")
}

fn text(cfgd: &mut Cfgd, name: &str) -> Option<String> {
    cfgd.get_string(&key(name)).filter(|value| !value.is_empty())
}

fn color(cfgd: &mut Cfgd, name: &str, default: u32) -> u32 {
    let Some(value) = text(cfgd, name) else {
        return default;
    };
    let parsed = value
        .strip_prefix('#')
        .filter(|digits| digits.len() == 6)
        .and_then(|digits| u32::from_str_radix(digits, 16).ok());
    match parsed {
        Some(color) => color,
        None => {
            tracing::warn!(key = name, value, "ignoring a color that is not in the form #rrggbb");
            default
        }
    }
}

fn int<T: TryFrom<i64>>(cfgd: &mut Cfgd, name: &str, default: T) -> T {
    cfgd.get_int(&key(name))
        .and_then(|value| T::try_from(value).ok())
        .unwrap_or(default)
}

impl Settings {
    pub fn load(cfgd: &mut Cfgd) -> Settings {
        let defaults = Settings::default();

        let mod_key = match text(cfgd, "mod-key").as_deref() {
            Some("super") => ModKeySetting::Super,
            Some("alt") => ModKeySetting::Alt,
            _ => ModKeySetting::Auto,
        };

        Settings {
            mod_key,
            terminal: text(cfgd, "terminal"),
            workspace_count: int(cfgd, "workspace-count", defaults.workspace_count),
            border_width: int(cfgd, "border-width", defaults.border_width),
            border_focused: color(cfgd, "border-color-focused", defaults.border_focused),
            border_unfocused: color(cfgd, "border-color-unfocused", defaults.border_unfocused),
            xkb: Xkb {
                rules: text(cfgd, "xkb-rules").unwrap_or_default(),
                model: text(cfgd, "xkb-model").unwrap_or_default(),
                layout: text(cfgd, "xkb-layout").unwrap_or_default(),
                variant: text(cfgd, "xkb-variant").unwrap_or_default(),
                options: text(cfgd, "xkb-options"),
            },
            repeat_delay: int(cfgd, "repeat-delay", defaults.repeat_delay),
            repeat_rate: int(cfgd, "repeat-rate", defaults.repeat_rate),
            cursor_theme: text(cfgd, "cursor-theme"),
            cursor_size: int(cfgd, "cursor-size", defaults.cursor_size),
            tap_to_click: cfgd.get_bool(&key("tap-to-click")).unwrap_or(defaults.tap_to_click),
            natural_scroll: cfgd.get_bool(&key("natural-scroll")).unwrap_or(defaults.natural_scroll),
        }
    }
}
