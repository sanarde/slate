// SPDX-License-Identifier: GPL-3.0-only

use crate::bindings::split_command;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
    Center,
    At(i32, i32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum RuleAction {
    Float,
    Tile,
    Workspace(usize),
    Size(u32, u32),
    Position(Position),
    Above,
    Sticky,
    Fullscreen,
    Maximize,
    Opacity(f32),
    NoShadow,
    NoDecoration,
    Blur,
    Scratchpad(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Field {
    AppId,
    Title,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Operator {
    Equals,
    Contains,
}

#[derive(Clone, Debug, PartialEq)]
struct Condition {
    field: Field,
    operator: Operator,
    pattern: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    conditions: Vec<Condition>,
    actions: Vec<RuleAction>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    pub floating: Option<bool>,
    pub workspace: Option<usize>,
    pub size: Option<(u32, u32)>,
    pub position: Option<Position>,
    pub above: bool,
    pub sticky: bool,
    pub fullscreen: bool,
    pub maximize: bool,
    pub opacity: Option<f32>,
    pub no_shadow: bool,
    pub no_decoration: bool,
    pub blur: bool,
    pub scratchpad: Option<String>,
}

fn glob(pattern: &[char], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|skip| glob(rest, &text[skip..])),
        Some(('?', rest)) => !text.is_empty() && glob(rest, &text[1..]),
        Some((character, rest)) => text.first() == Some(character) && glob(rest, &text[1..]),
    }
}

impl Condition {
    fn matches(&self, app_id: &str, title: &str) -> bool {
        let value = match self.field {
            Field::AppId => app_id,
            Field::Title => title,
        };
        match self.operator {
            Operator::Equals => {
                let pattern: Vec<char> = self.pattern.chars().collect();
                let text: Vec<char> = value.chars().collect();
                glob(&pattern, &text)
            }
            Operator::Contains => value.contains(&self.pattern),
        }
    }
}

impl Rule {
    pub fn matches(&self, app_id: &str, title: &str) -> bool {
        self.conditions.iter().all(|condition| condition.matches(app_id, title))
    }

    pub fn uses_title(&self) -> bool {
        self.conditions.iter().any(|condition| condition.field == Field::Title)
    }
}

fn parse_condition(token: &str) -> Result<Condition, String> {
    let (key, operator, pattern) = if let Some((key, pattern)) = token.split_once("~=") {
        (key, Operator::Contains, pattern)
    } else if let Some((key, pattern)) = token.split_once('=') {
        (key, Operator::Equals, pattern)
    } else {
        return Err(format!("{token} is not a condition"));
    };

    let field = match key {
        "app_id" | "class" => Field::AppId,
        "title" => Field::Title,
        other => return Err(format!("unknown condition {other}")),
    };
    Ok(Condition {
        field,
        operator,
        pattern: pattern.to_string(),
    })
}

fn parse_workspace(text: &str) -> Result<usize, String> {
    let number: usize = text.trim().parse().map_err(|_| "the workspace is not a number".to_string())?;
    if number == 0 {
        return Err("workspaces are numbered from 1".to_string());
    }
    Ok(number - 1)
}

fn parse_action(text: &str) -> Result<RuleAction, String> {
    let text = text.trim();
    let (name, argument) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let argument = argument.trim();

    Ok(match name {
        "float" => RuleAction::Float,
        "tile" => RuleAction::Tile,
        "above" => RuleAction::Above,
        "sticky" => RuleAction::Sticky,
        "fullscreen" => RuleAction::Fullscreen,
        "maximize" => RuleAction::Maximize,
        "no-shadow" => RuleAction::NoShadow,
        "no-decoration" => RuleAction::NoDecoration,
        "blur" => RuleAction::Blur,
        "workspace" => RuleAction::Workspace(parse_workspace(argument)?),
        "scratchpad" => {
            if argument.is_empty() {
                return Err("scratchpad needs a name".to_string());
            }
            RuleAction::Scratchpad(argument.to_string())
        }
        "opacity" => {
            let value: f32 = argument.parse().map_err(|_| "the opacity is not a number".to_string())?;
            if !(0.05..=1.0).contains(&value) {
                return Err("the opacity is between 0.05 and 1".to_string());
            }
            RuleAction::Opacity(value)
        }
        "size" => {
            let (width, height) = argument.split_once('x').ok_or("the size is WIDTHxHEIGHT")?;
            let width: u32 = width.trim().parse().map_err(|_| "the width is not a number".to_string())?;
            let height: u32 = height.trim().parse().map_err(|_| "the height is not a number".to_string())?;
            if width == 0 || height == 0 {
                return Err("the size is not positive".to_string());
            }
            RuleAction::Size(width, height)
        }
        "position" => {
            if argument == "center" {
                RuleAction::Position(Position::Center)
            } else {
                let (x, y) = argument.split_once(',').ok_or("the position is X,Y or center")?;
                RuleAction::Position(Position::At(
                    x.trim().parse().map_err(|_| "x is not a number".to_string())?,
                    y.trim().parse().map_err(|_| "y is not a number".to_string())?,
                ))
            }
        }
        other => return Err(format!("unknown action {other}")),
    })
}

fn split_actions(text: &str) -> Vec<String> {
    let mut actions: Vec<String> = Vec::new();
    for piece in text.split(',').map(str::trim).filter(|piece| !piece.is_empty()) {
        let continues = piece.starts_with(|character: char| character.is_ascii_digit() || character == '-');
        match actions.last_mut() {
            Some(previous) if continues => {
                previous.push(',');
                previous.push_str(piece);
            }
            _ => actions.push(piece.to_string()),
        }
    }
    actions
}

pub fn parse(entry: &str) -> Result<Rule, String> {
    let (left, right) = entry.split_once("=>").ok_or("expected CONDITIONS => ACTIONS")?;

    let tokens = split_command(left)?;
    if tokens.is_empty() {
        return Err("a rule needs at least one condition".to_string());
    }
    let conditions = tokens
        .iter()
        .map(|token| parse_condition(token))
        .collect::<Result<Vec<_>, _>>()?;

    let actions = split_actions(right)
        .iter()
        .map(|action| parse_action(action))
        .collect::<Result<Vec<_>, _>>()?;
    if actions.is_empty() {
        return Err("a rule needs at least one action".to_string());
    }
    Ok(Rule { conditions, actions })
}

pub fn parse_all(entries: &[String]) -> Vec<Rule> {
    entries
        .iter()
        .filter_map(|entry| match parse(entry) {
            Ok(rule) => Some(rule),
            Err(error) => {
                tracing::warn!(entry = entry.as_str(), error, "ignoring a window rule");
                None
            }
        })
        .collect()
}

pub fn apply(rules: &[Rule], app_id: &str, title: &str) -> Applied {
    let mut applied = Applied::default();

    for rule in rules.iter().filter(|rule| rule.matches(app_id, title)) {
        for action in &rule.actions {
            match action {
                RuleAction::Float => applied.floating = Some(true),
                RuleAction::Tile => applied.floating = Some(false),
                RuleAction::Workspace(index) => applied.workspace = Some(*index),
                RuleAction::Size(width, height) => applied.size = Some((*width, *height)),
                RuleAction::Position(position) => applied.position = Some(*position),
                RuleAction::Above => applied.above = true,
                RuleAction::Sticky => applied.sticky = true,
                RuleAction::Fullscreen => applied.fullscreen = true,
                RuleAction::Maximize => applied.maximize = true,
                RuleAction::Opacity(value) => applied.opacity = Some(*value),
                RuleAction::NoShadow => applied.no_shadow = true,
                RuleAction::NoDecoration => applied.no_decoration = true,
                RuleAction::Blur => applied.blur = true,
                RuleAction::Scratchpad(name) => applied.scratchpad = Some(name.clone()),
            }
        }
    }
    applied
}

pub fn parse_scratchpads(entries: &[String]) -> Vec<(String, Vec<String>)> {
    entries
        .iter()
        .filter_map(|entry| {
            let (name, command) = entry.split_once('=')?;
            let command = split_command(command).ok()?;
            let name = name.trim();
            (!name.is_empty() && !command.is_empty()).then(|| (name.to_string(), command))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_parse_with_conditions_and_actions() {
        let rule = parse("app_id=firefox title~=\"Picture in Picture\" => float, size 480x270, position 20,30, above, sticky, opacity 0.9").unwrap();
        assert_eq!(rule.conditions.len(), 2);
        assert!(rule.uses_title());
        assert_eq!(
            rule.actions,
            vec![
                RuleAction::Float,
                RuleAction::Size(480, 270),
                RuleAction::Position(Position::At(20, 30)),
                RuleAction::Above,
                RuleAction::Sticky,
                RuleAction::Opacity(0.9),
            ]
        );
        assert_eq!(parse("app_id=x => workspace 3, position center, scratchpad term").unwrap().actions.len(), 3);
    }

    #[test]
    fn bad_rules_are_rejected() {
        for entry in [
            "no arrow",
            "=> float",
            "app_id=x =>",
            "color=red => float",
            "app_id=x => dance",
            "app_id=x => workspace 0",
            "app_id=x => opacity 2",
            "app_id=x => size 10",
            "app_id=x => position 1",
        ] {
            assert!(parse(entry).is_err(), "{entry}");
        }
    }

    #[test]
    fn matching_uses_globs_and_substrings() {
        let rules = parse_all(&[
            "app_id=fire* => float".to_string(),
            "title~=Calc => workspace 2".to_string(),
            "app_id=?oot title~=zsh => tile, no-shadow".to_string(),
        ]);

        let firefox = apply(&rules, "firefox", "Mozilla");
        assert_eq!(firefox.floating, Some(true));
        assert_eq!(firefox.workspace, None);

        let calculator = apply(&rules, "calc", "GNOME Calculator");
        assert_eq!(calculator.workspace, Some(1));

        let terminal = apply(&rules, "xterm", "~ zsh");
        assert_eq!(terminal.floating, None);
        let both = apply(&rules, "boot", "zsh");
        assert_eq!(both.floating, Some(false));
        assert!(both.no_shadow);
    }

    #[test]
    fn later_rules_win() {
        let rules = parse_all(&["app_id=a => float, opacity 0.5".to_string(), "app_id=a => tile, opacity 0.8".to_string()]);
        let applied = apply(&rules, "a", "");
        assert_eq!(applied.floating, Some(false));
        assert_eq!(applied.opacity, Some(0.8));
    }

    #[test]
    fn scratchpads_have_names_and_commands() {
        let pads = parse_scratchpads(&["term = foot --app-id scratch".to_string(), "bad".to_string(), "= x".to_string()]);
        assert_eq!(pads, vec![("term".to_string(), vec!["foot".to_string(), "--app-id".to_string(), "scratch".to_string()])]);
    }
}
