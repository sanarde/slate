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
        let digits = text.strip_prefix('#')?;
        let value = u32::from_str_radix(digits, 16).ok()?;
        match digits.len() {
            6 => Some(Color::rgb(value)),
            8 => Some(Color::rgba(value)),
            _ => None,
        }
    }
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
}

#[derive(Clone, Copy, PartialEq)]
pub enum ClickAction {
    None,
    Maximize,
    Fullscreen,
    Close,
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
    pub button_close_hover_color: Color,
    pub button_icon_color_focused: Color,
    pub button_icon_color_unfocused: Color,
    pub double_click: ClickAction,
    pub middle_click: ClickAction,
    pub right_click: ClickAction,
}

impl Default for Decoration {
    fn default() -> Decoration {
        Decoration {
            mode: DecorationMode::Server,
            rules: Vec::new(),
            titlebar_height: 30,
            titlebar_color_focused: Color::rgb(0x6750a4),
            titlebar_color_unfocused: Color::rgb(0x49454f),
            title_color_focused: Color::rgb(0xffffff),
            title_color_unfocused: Color::rgb(0xcac4d0),
            title_font: "sans-serif".to_string(),
            title_font_size: 13,
            title_align: Align::Center,
            title_padding: 12,
            corner_radius: 8,
            resize_margin: 4,
            buttons_left: Vec::new(),
            buttons_right: vec![ButtonKind::Maximize, ButtonKind::Close],
            button_size: 18,
            button_spacing: 6,
            button_margin: 8,
            button_shape: ButtonShape::Circle,
            button_color: Color::rgba(0x00000000),
            button_hover_color: Color::rgba(0xffffff33),
            button_close_hover_color: Color::rgb(0xe5484d),
            button_icon_color_focused: Color::rgb(0xffffff),
            button_icon_color_unfocused: Color::rgb(0xcac4d0),
            double_click: ClickAction::Maximize,
            middle_click: ClickAction::None,
            right_click: ClickAction::None,
        }
    }
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
    pub decoration: Decoration,
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
            decoration: Decoration::default(),
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

fn color_value(cfgd: &mut Cfgd, name: &str, default: Color) -> Color {
    let Some(value) = text(cfgd, name) else {
        return default;
    };
    Color::parse(&value).unwrap_or_else(|| {
        tracing::warn!(key = name, value, "ignoring a color that is not in the form #rrggbb or #rrggbbaa");
        default
    })
}

fn choice<T: Copy>(cfgd: &mut Cfgd, name: &str, default: T, choices: &[(&str, T)]) -> T {
    let Some(value) = text(cfgd, name) else {
        return default;
    };
    choices
        .iter()
        .find(|(candidate, _)| *candidate == value)
        .map(|(_, choice)| *choice)
        .unwrap_or(default)
}

fn buttons(cfgd: &mut Cfgd, defaults: &Decoration) -> (Vec<ButtonKind>, Vec<ButtonKind>) {
    let Some(layout) = cfgd.get_string(&key("button-layout")) else {
        return (defaults.buttons_left.clone(), defaults.buttons_right.clone());
    };

    let parse = |side: &str| -> Vec<ButtonKind> {
        side.split(',')
            .filter_map(|item| match item.trim() {
                "close" => Some(ButtonKind::Close),
                "maximize" => Some(ButtonKind::Maximize),
                "" => None,
                other => {
                    tracing::warn!(button = other, "ignoring an unknown titlebar button");
                    None
                }
            })
            .collect()
    };
    let (left, right) = layout.split_once(':').unwrap_or(("", layout.as_str()));
    (parse(left), parse(right))
}

fn rules(cfgd: &mut Cfgd) -> Vec<(String, RuleMode)> {
    cfgd.get_strv(&key("decoration-rules"))
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            let (app_id, mode) = entry.split_once('=')?;
            let mode = match mode.trim() {
                "server" => RuleMode::Server,
                "client" => RuleMode::Client,
                "none" => RuleMode::None,
                _ => return None,
            };
            Some((app_id.trim().to_string(), mode))
        })
        .collect()
}

fn load_decoration(cfgd: &mut Cfgd) -> Decoration {
    let defaults = Decoration::default();
    let (buttons_left, buttons_right) = buttons(cfgd, &defaults);
    let actions = |name: &str, default: ClickAction, cfgd: &mut Cfgd| {
        choice(
            cfgd,
            name,
            default,
            &[
                ("none", ClickAction::None),
                ("maximize", ClickAction::Maximize),
                ("fullscreen", ClickAction::Fullscreen),
                ("close", ClickAction::Close),
            ],
        )
    };

    Decoration {
        mode: choice(
            cfgd,
            "decoration-mode",
            defaults.mode,
            &[
                ("server", DecorationMode::Server),
                ("client", DecorationMode::Client),
                ("auto", DecorationMode::Auto),
            ],
        ),
        rules: rules(cfgd),
        titlebar_height: int(cfgd, "titlebar-height", defaults.titlebar_height),
        titlebar_color_focused: color_value(cfgd, "titlebar-color-focused", defaults.titlebar_color_focused),
        titlebar_color_unfocused: color_value(cfgd, "titlebar-color-unfocused", defaults.titlebar_color_unfocused),
        title_color_focused: color_value(cfgd, "title-color-focused", defaults.title_color_focused),
        title_color_unfocused: color_value(cfgd, "title-color-unfocused", defaults.title_color_unfocused),
        title_font: text(cfgd, "title-font").unwrap_or(defaults.title_font.clone()),
        title_font_size: int(cfgd, "title-font-size", defaults.title_font_size),
        title_align: choice(
            cfgd,
            "title-align",
            defaults.title_align,
            &[("left", Align::Left), ("center", Align::Center), ("right", Align::Right)],
        ),
        title_padding: int(cfgd, "title-padding", defaults.title_padding),
        corner_radius: int(cfgd, "corner-radius", defaults.corner_radius),
        resize_margin: int(cfgd, "resize-margin", defaults.resize_margin),
        buttons_left,
        buttons_right,
        button_size: int(cfgd, "button-size", defaults.button_size),
        button_spacing: int(cfgd, "button-spacing", defaults.button_spacing),
        button_margin: int(cfgd, "button-margin", defaults.button_margin),
        button_shape: choice(
            cfgd,
            "button-shape",
            defaults.button_shape,
            &[
                ("circle", ButtonShape::Circle),
                ("rounded", ButtonShape::Rounded),
                ("square", ButtonShape::Square),
            ],
        ),
        button_color: color_value(cfgd, "button-color", defaults.button_color),
        button_hover_color: color_value(cfgd, "button-hover-color", defaults.button_hover_color),
        button_close_hover_color: color_value(cfgd, "button-close-hover-color", defaults.button_close_hover_color),
        button_icon_color_focused: color_value(cfgd, "button-icon-color-focused", defaults.button_icon_color_focused),
        button_icon_color_unfocused: color_value(
            cfgd,
            "button-icon-color-unfocused",
            defaults.button_icon_color_unfocused,
        ),
        double_click: actions("titlebar-double-click", defaults.double_click, cfgd),
        middle_click: actions("titlebar-middle-click", defaults.middle_click, cfgd),
        right_click: actions("titlebar-right-click", defaults.right_click, cfgd),
    }
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
            decoration: load_decoration(cfgd),
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
}
