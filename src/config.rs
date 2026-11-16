// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use crate::bindings::{self, Action, Binding};
use crate::layout::Layout;
use crate::rules::{self, Rule};

pub const PREFIX: &str = "/org/nuros/sanar/slate/";
pub const MAX_WORKSPACES: usize = 9;

pub const LIST_KEYS: [&str; 6] = [
    "bindings",
    "decoration-rules",
    "floating-rules",
    "window-rules",
    "scratchpads",
    "exec-once",
];

pub const DEFAULTS: &[(&str, &str)] = &[
    ("theme", "material-you"),
    ("color-scheme", "auto"),
    ("layout", "floating"),
    ("tiling-gap", "8"),
    ("tiling-outer-gap", "8"),
    ("master-ratio", "55"),
    ("master-count", "1"),
    ("shadow-size", "18"),
    ("shadow-offset-x", "0"),
    ("shadow-offset-y", "6"),
    ("shadow-color-focused", "#00000073"),
    ("shadow-color-unfocused", "#00000040"),
    ("shadow-tiled", "false"),
    ("elevation-focused", "-1"),
    ("elevation-unfocused", "-1"),
    ("animations", "true"),
    ("animation-duration", "200"),
    ("animation-curve", "ease-out"),
    ("animation-close-curve", "ease-out"),
    ("animation-close-duration", "200"),
    ("animate-open", "true"),
    ("animate-move", "true"),
    ("animate-workspace", "true"),
    ("animate-close", "true"),
    ("animation-open-scale", "94"),
    ("round-content", "true"),
    ("dim-inactive", "0"),
    ("blur-radius", "3"),
    ("blur-passes", "2"),
    ("mod-key", "auto"),
    ("terminal", ""),
    ("workspace-count", "9"),
    ("border-width", "2"),
    ("border-color-focused", "#6750a4"),
    ("border-color-unfocused", "#49454f"),
    ("xkb-rules", ""),
    ("xkb-model", ""),
    ("xkb-layout", ""),
    ("xkb-variant", ""),
    ("xkb-options", ""),
    ("repeat-delay", "200"),
    ("repeat-rate", "25"),
    ("cursor-theme", ""),
    ("cursor-size", "0"),
    ("tap-to-click", "true"),
    ("natural-scroll", "false"),
    ("gesture-fingers", "3"),
    ("gesture-threshold", "100"),
    ("gesture-swipe-left", "workspace-next"),
    ("gesture-swipe-right", "workspace-prev"),
    ("gesture-swipe-up", "none"),
    ("gesture-swipe-down", "none"),
    ("xdg-autostart", "false"),
    ("session-environment", "true"),
    ("decoration-mode", "server"),
    ("titlebar-height", "30"),
    ("titlebar-color-focused", "#6750a4"),
    ("titlebar-color-unfocused", "#49454f"),
    ("title-color-focused", "#ffffff"),
    ("title-color-unfocused", "#cac4d0"),
    ("title-font", "sans-serif"),
    ("button-icon-font", ""),
    ("title-font-size", "13"),
    ("title-align", "center"),
    ("title-padding", "12"),
    ("corner-radius", "8"),
    ("resize-margin", "4"),
    ("button-layout", ":minimize,maximize,close"),
    ("button-size", "18"),
    ("button-spacing", "6"),
    ("button-margin", "8"),
    ("button-shape", "circle"),
    ("button-color", "#00000000"),
    ("button-hover-color", "#ffffff33"),
    ("button-pressed-color", "#ffffff4d"),
    ("button-close-hover-color", "#e5484d"),
    ("button-icon-color-focused", "#ffffff"),
    ("button-icon-color-unfocused", "#cac4d0"),
    ("titlebar-double-click", "maximize"),
    ("titlebar-middle-click", "none"),
    ("titlebar-right-click", "none"),
];

pub fn default_of(key: &str) -> &'static str {
    DEFAULTS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| *value)
        .unwrap_or("")
}

pub fn is_known(key: &str) -> bool {
    LIST_KEYS.contains(&key) || DEFAULTS.iter().any(|(name, _)| *name == key)
}

#[derive(Clone, Default)]
pub struct Values {
    pub scalars: HashMap<String, String>,
    pub lists: HashMap<String, Vec<String>>,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(value: u32) -> Color {
        Color {
            r: (value >> 16) as u8,
            g: (value >> 8) as u8,
            b: value as u8,
            a: 255,
        }
    }

    pub const fn rgba(value: u32) -> Color {
        Color {
            r: (value >> 24) as u8,
            g: (value >> 16) as u8,
            b: (value >> 8) as u8,
            a: value as u8,
        }
    }

    pub fn parse(text: &str) -> Option<Color> {
        let digits = text.trim().strip_prefix('#')?;
        let value = u32::from_str_radix(digits, 16).ok()?;
        match digits.len() {
            6 => Some(Color::rgb(value)),
            8 => Some(Color::rgba(value)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ModKeySetting {
    Auto,
    Super,
    Alt,
}

#[derive(Clone, Copy, PartialEq)]
pub enum DecorationMode {
    Server,
    Client,
    Auto,
}

#[derive(Clone, Copy, PartialEq)]
pub enum RuleMode {
    Server,
    Client,
    None,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ButtonShape {
    Circle,
    Rounded,
    Square,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ButtonKind {
    Close,
    Maximize,
    Minimize,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ClickAction {
    None,
    Maximize,
    Minimize,
    Fullscreen,
    Close,
}

#[derive(Clone, PartialEq)]
pub struct Tiling {
    pub layout: Layout,
    pub gap: i32,
    pub outer_gap: i32,
    pub master_ratio: f32,
    pub master_count: usize,
    pub floating_rules: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Curve {
    Linear,
    EaseOut,
    EaseInOut,
    Overshoot,
    Standard,
    StandardDecelerate,
    StandardAccelerate,
    Emphasized,
    EmphasizedDecelerate,
    EmphasizedAccelerate,
}

#[derive(Clone, PartialEq)]
pub struct Shadow {
    pub size: i32,
    pub offset: (i32, i32),
    pub color: Color,
}

#[derive(Clone, PartialEq)]
pub struct Effects {
    pub shadow_focused: Shadow,
    pub shadow_unfocused: Shadow,
    pub shadow_tiled: bool,
    pub animations: bool,
    pub duration_ms: u64,
    pub curve: Curve,
    pub close_curve: Curve,
    pub close_duration_ms: u64,
    pub animate_open: bool,
    pub animate_move: bool,
    pub animate_workspace: bool,
    pub animate_close: bool,
    pub open_scale: f32,
    pub round_content: bool,
    pub dim_inactive: f32,
    pub blur_radius: i32,
    pub blur_passes: i32,
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
pub struct Decoration {
    pub mode: DecorationMode,
    pub rules: Vec<(String, RuleMode)>,
    pub titlebar_height: u32,
    pub titlebar_color_focused: Color,
    pub titlebar_color_unfocused: Color,
    pub title_color_focused: Color,
    pub title_color_unfocused: Color,
    pub title_font: String,
    pub icon_font: String,
    pub title_font_size: u32,
    pub title_align: Align,
    pub title_padding: u32,
    pub corner_radius: u32,
    pub resize_margin: u32,
    pub buttons_left: Vec<ButtonKind>,
    pub buttons_right: Vec<ButtonKind>,
    pub button_size: u32,
    pub button_spacing: u32,
    pub button_margin: u32,
    pub button_shape: ButtonShape,
    pub button_color: Color,
    pub button_hover_color: Color,
    pub button_pressed_color: Color,
    pub button_close_hover_color: Color,
    pub button_icon_color_focused: Color,
    pub button_icon_color_unfocused: Color,
    pub double_click: ClickAction,
    pub middle_click: ClickAction,
    pub right_click: ClickAction,
}

#[derive(Clone, PartialEq)]
pub struct Gestures {
    pub fingers: u32,
    pub threshold: f64,
    pub left: Option<Action>,
    pub right: Option<Action>,
    pub up: Option<Action>,
    pub down: Option<Action>,
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
    pub gestures: Gestures,
    pub decoration: Decoration,
    pub tiling: Tiling,
    pub effects: Effects,
    pub bindings: Vec<Binding>,
    pub window_rules: Vec<Rule>,
    pub scratchpads: Vec<(String, Vec<String>)>,
    pub exec_once: Vec<Vec<String>>,
    pub xdg_autostart: bool,
    pub session_environment: bool,
}

fn parsed<T>(values: &Values, key: &str, parse: impl Fn(&str) -> Option<T>) -> T {
    if let Some(text) = values.scalars.get(key) {
        if let Some(value) = parse(text) {
            return value;
        }
        tracing::warn!(key, value = text.as_str(), "ignoring an invalid value");
    }
    parse(default_of(key)).expect("the built in defaults are valid")
}

fn int(values: &Values, key: &str, min: i64, max: i64) -> i64 {
    parsed(values, key, |text| text.trim().parse::<i64>().ok()).clamp(min, max)
}

fn boolean(values: &Values, key: &str) -> bool {
    parsed(values, key, |text| match text.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

pub fn elevation(level: i64) -> Shadow {
    const SIZE: [i32; 6] = [0, 6, 10, 14, 18, 22];
    const OFFSET: [i32; 6] = [0, 1, 3, 5, 7, 9];
    const ALPHA: [u8; 6] = [0, 46, 56, 66, 77, 87];
    let index = level.clamp(0, 5) as usize;
    Shadow {
        size: SIZE[index],
        offset: (0, OFFSET[index]),
        color: Color::rgba(u32::from(ALPHA[index])),
    }
}

fn shadow(values: &Values, elevation_key: &str, color_key: &str) -> Shadow {
    let level = int(values, elevation_key, -1, 5);
    if level >= 0 {
        return elevation(level);
    }
    Shadow {
        size: int(values, "shadow-size", 0, 128) as i32,
        offset: (
            int(values, "shadow-offset-x", -64, 64) as i32,
            int(values, "shadow-offset-y", -64, 64) as i32,
        ),
        color: color(values, color_key),
    }
}

fn gesture_action(values: &Values, key: &str) -> Option<Action> {
    parsed(values, key, |text| bindings::parse_action(text).ok())
}

fn color(values: &Values, key: &str) -> Color {
    parsed(values, key, Color::parse)
}

fn optional(values: &Values, key: &str) -> Option<String> {
    let text = values.scalars.get(key).map(|text| text.trim()).unwrap_or(default_of(key));
    (!text.is_empty()).then(|| text.to_string())
}

fn text(values: &Values, key: &str) -> String {
    optional(values, key).unwrap_or_default()
}

fn choice<T: Copy>(values: &Values, key: &str, choices: &[(&str, T)]) -> T {
    parsed(values, key, |text| {
        choices
            .iter()
            .find(|(candidate, _)| *candidate == text.trim())
            .map(|(_, choice)| *choice)
    })
}

fn rgb_value(color: Color) -> u32 {
    (color.r as u32) << 16 | (color.g as u32) << 8 | color.b as u32
}

fn buttons(layout: &str) -> (Vec<ButtonKind>, Vec<ButtonKind>) {
    let parse = |side: &str| -> Vec<ButtonKind> {
        side.split(',')
            .filter_map(|item| match item.trim() {
                "close" => Some(ButtonKind::Close),
                "maximize" => Some(ButtonKind::Maximize),
                "minimize" => Some(ButtonKind::Minimize),
                "" => None,
                other => {
                    tracing::warn!(button = other, "ignoring an unknown titlebar button");
                    None
                }
            })
            .collect()
    };
    let (left, right) = layout.split_once(':').unwrap_or(("", layout));
    (parse(left), parse(right))
}

fn rules(values: &Values) -> Vec<(String, RuleMode)> {
    values
        .lists
        .get("decoration-rules")
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            let (app_id, mode) = entry.split_once('=')?;
            let mode = match mode.trim() {
                "server" => RuleMode::Server,
                "client" => RuleMode::Client,
                "none" => RuleMode::None,
                _ => {
                    tracing::warn!(entry = entry.as_str(), "ignoring a decoration rule");
                    return None;
                }
            };
            Some((app_id.trim().to_string(), mode))
        })
        .collect()
}

fn decoration_from(values: &Values) -> Decoration {
    let actions = [
        ("none", ClickAction::None),
        ("maximize", ClickAction::Maximize),
        ("minimize", ClickAction::Minimize),
        ("fullscreen", ClickAction::Fullscreen),
        ("close", ClickAction::Close),
    ];
    let (buttons_left, buttons_right) = buttons(&text(values, "button-layout"));

    Decoration {
        mode: choice(
            values,
            "decoration-mode",
            &[
                ("server", DecorationMode::Server),
                ("client", DecorationMode::Client),
                ("auto", DecorationMode::Auto),
            ],
        ),
        rules: rules(values),
        titlebar_height: int(values, "titlebar-height", 0, 96) as u32,
        titlebar_color_focused: color(values, "titlebar-color-focused"),
        titlebar_color_unfocused: color(values, "titlebar-color-unfocused"),
        title_color_focused: color(values, "title-color-focused"),
        title_color_unfocused: color(values, "title-color-unfocused"),
        title_font: text(values, "title-font"),
        icon_font: text(values, "button-icon-font"),
        title_font_size: int(values, "title-font-size", 6, 64) as u32,
        title_align: choice(
            values,
            "title-align",
            &[("left", Align::Left), ("center", Align::Center), ("right", Align::Right)],
        ),
        title_padding: int(values, "title-padding", 0, 128) as u32,
        corner_radius: int(values, "corner-radius", 0, 32) as u32,
        resize_margin: int(values, "resize-margin", 0, 32) as u32,
        buttons_left,
        buttons_right,
        button_size: int(values, "button-size", 8, 64) as u32,
        button_spacing: int(values, "button-spacing", 0, 32) as u32,
        button_margin: int(values, "button-margin", 0, 64) as u32,
        button_shape: choice(
            values,
            "button-shape",
            &[
                ("circle", ButtonShape::Circle),
                ("rounded", ButtonShape::Rounded),
                ("square", ButtonShape::Square),
            ],
        ),
        button_color: color(values, "button-color"),
        button_hover_color: color(values, "button-hover-color"),
        button_pressed_color: color(values, "button-pressed-color"),
        button_close_hover_color: color(values, "button-close-hover-color"),
        button_icon_color_focused: color(values, "button-icon-color-focused"),
        button_icon_color_unfocused: color(values, "button-icon-color-unfocused"),
        double_click: choice(values, "titlebar-double-click", &actions),
        middle_click: choice(values, "titlebar-middle-click", &actions),
        right_click: choice(values, "titlebar-right-click", &actions),
    }
}

impl Default for Decoration {
    fn default() -> Decoration {
        decoration_from(&Values::default())
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings::from_values(&Values::default())
    }
}

impl Settings {
    pub fn from_values(values: &Values) -> Settings {
        Settings {
            mod_key: choice(
                values,
                "mod-key",
                &[
                    ("auto", ModKeySetting::Auto),
                    ("super", ModKeySetting::Super),
                    ("alt", ModKeySetting::Alt),
                ],
            ),
            terminal: optional(values, "terminal"),
            workspace_count: int(values, "workspace-count", 1, MAX_WORKSPACES as i64) as usize,
            border_width: int(values, "border-width", 0, 32) as u32,
            border_focused: rgb_value(color(values, "border-color-focused")),
            border_unfocused: rgb_value(color(values, "border-color-unfocused")),
            xkb: Xkb {
                rules: text(values, "xkb-rules"),
                model: text(values, "xkb-model"),
                layout: text(values, "xkb-layout"),
                variant: text(values, "xkb-variant"),
                options: optional(values, "xkb-options"),
            },
            repeat_delay: int(values, "repeat-delay", 10, 2000) as i32,
            repeat_rate: int(values, "repeat-rate", 1, 200) as i32,
            cursor_theme: optional(values, "cursor-theme"),
            cursor_size: int(values, "cursor-size", 0, 256) as u32,
            tap_to_click: boolean(values, "tap-to-click"),
            natural_scroll: boolean(values, "natural-scroll"),
            gestures: Gestures {
                fingers: int(values, "gesture-fingers", 3, 4) as u32,
                threshold: int(values, "gesture-threshold", 20, 500) as f64,
                left: gesture_action(values, "gesture-swipe-left"),
                right: gesture_action(values, "gesture-swipe-right"),
                up: gesture_action(values, "gesture-swipe-up"),
                down: gesture_action(values, "gesture-swipe-down"),
            },
            decoration: decoration_from(values),
            tiling: Tiling {
                layout: choice(
                    values,
                    "layout",
                    &[
                        ("floating", Layout::Floating),
                        ("master-stack", Layout::MasterStack),
                        ("spiral", Layout::Spiral),
                        ("grid", Layout::Grid),
                        ("monocle", Layout::Monocle),
                    ],
                ),
                gap: int(values, "tiling-gap", 0, 64) as i32,
                outer_gap: int(values, "tiling-outer-gap", 0, 128) as i32,
                master_ratio: int(values, "master-ratio", 10, 90) as f32 / 100.0,
                master_count: int(values, "master-count", 1, 8) as usize,
                floating_rules: values.lists.get("floating-rules").cloned().unwrap_or_default(),
            },
            effects: Effects {
                shadow_focused: shadow(values, "elevation-focused", "shadow-color-focused"),
                shadow_unfocused: shadow(values, "elevation-unfocused", "shadow-color-unfocused"),
                shadow_tiled: boolean(values, "shadow-tiled"),
                animations: boolean(values, "animations"),
                duration_ms: int(values, "animation-duration", 0, 2000) as u64,
                curve: choice(
                    values,
                    "animation-curve",
                    &[
                        ("linear", Curve::Linear),
                        ("ease-out", Curve::EaseOut),
                        ("ease-in-out", Curve::EaseInOut),
                        ("overshoot", Curve::Overshoot),
                        ("standard", Curve::Standard),
                        ("standard-decelerate", Curve::StandardDecelerate),
                        ("standard-accelerate", Curve::StandardAccelerate),
                        ("emphasized", Curve::Emphasized),
                        ("emphasized-decelerate", Curve::EmphasizedDecelerate),
                        ("emphasized-accelerate", Curve::EmphasizedAccelerate),
                    ],
                ),
                close_curve: choice(
                    values,
                    "animation-close-curve",
                    &[
                        ("linear", Curve::Linear),
                        ("ease-out", Curve::EaseOut),
                        ("ease-in-out", Curve::EaseInOut),
                        ("overshoot", Curve::Overshoot),
                        ("standard", Curve::Standard),
                        ("standard-decelerate", Curve::StandardDecelerate),
                        ("standard-accelerate", Curve::StandardAccelerate),
                        ("emphasized", Curve::Emphasized),
                        ("emphasized-decelerate", Curve::EmphasizedDecelerate),
                        ("emphasized-accelerate", Curve::EmphasizedAccelerate),
                    ],
                ),
                close_duration_ms: int(values, "animation-close-duration", 0, 2000) as u64,
                animate_open: boolean(values, "animate-open"),
                animate_move: boolean(values, "animate-move"),
                animate_workspace: boolean(values, "animate-workspace"),
                animate_close: boolean(values, "animate-close"),
                open_scale: int(values, "animation-open-scale", 50, 100) as f32 / 100.0,
                round_content: boolean(values, "round-content"),
                dim_inactive: int(values, "dim-inactive", 0, 80) as f32 / 100.0,
                blur_radius: int(values, "blur-radius", 0, 32) as i32,
                blur_passes: int(values, "blur-passes", 1, 4) as i32,
            },
            bindings: bindings::parse(values.lists.get("bindings").map(Vec::as_slice).unwrap_or_default()),
            window_rules: rules::parse_all(values.lists.get("window-rules").map(Vec::as_slice).unwrap_or_default()),
            scratchpads: rules::parse_scratchpads(values.lists.get("scratchpads").map(Vec::as_slice).unwrap_or_default()),
            exec_once: values
                .lists
                .get("exec-once")
                .map(|entries| entries.iter().filter_map(|entry| bindings::split_command(entry).ok()).filter(|command| !command.is_empty()).collect())
                .unwrap_or_default(),
            xdg_autostart: boolean(values, "xdg-autostart"),
            session_environment: boolean(values, "session-environment"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_with_and_without_alpha() {
        assert!(Color::parse("#6750a4") == Some(Color::rgb(0x6750a4)));
        assert!(Color::parse("#ffffff33") == Some(Color::rgba(0xffffff33)));
        assert!(Color::parse("6750a4").is_none());
        assert!(Color::parse("#fff").is_none());
        assert!(Color::parse("#gggggg").is_none());
    }

    #[test]
    fn the_schema_and_the_defaults_agree() {
        let schema = include_str!("../data/org.nuros.sanar.slate.schema");
        let mut section = None;
        let mut seen = Vec::new();

        for line in schema.lines().map(str::trim) {
            if let Some(name) = line.strip_prefix('[').and_then(|line| line.strip_suffix(']')) {
                section = (name != "schema").then(|| name.to_string());
            } else if let (Some(name), Some(default)) = (&section, line.strip_prefix("default =")) {
                if LIST_KEYS.contains(&name.as_str()) {
                    continue;
                }
                assert_eq!(default.trim(), default_of(name), "{name}");
                seen.push(name.clone());
            }
        }

        for (name, _) in DEFAULTS {
            assert!(seen.iter().any(|candidate| candidate == name), "{name} is missing in the schema");
        }
    }

    #[test]
    fn explicit_values_override_the_defaults_and_bad_ones_fall_back() {
        let mut values = Values::default();
        values.scalars.insert("titlebar-height".into(), "40".into());
        values.scalars.insert("border-width".into(), "wide".into());
        values.scalars.insert("title-align".into(), "left".into());

        let settings = Settings::from_values(&values);
        assert_eq!(settings.decoration.titlebar_height, 40);
        assert_eq!(settings.border_width, 2);
        assert!(settings.decoration.title_align == Align::Left);
    }
}
