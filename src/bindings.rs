// SPDX-License-Identifier: GPL-3.0-only

use smithay::input::keyboard::xkb;

use crate::monitors::Direction;

pub const WORKSPACE_KEYS: usize = 9;

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Modifiers {
    pub primary: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Required {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

#[derive(Clone, PartialEq)]
pub enum Action {
    Terminal,
    Close,
    Maximize,
    Minimize,
    RestoreMinimized,
    Fullscreen,
    Quit,
    Workspace(usize),
    MoveToWorkspace(usize),
    FocusNext,
    FocusPrevious,
    ToggleTiling,
    LayoutNext,
    ToggleFloating,
    MasterGrow,
    MasterShrink,
    MasterMore,
    MasterFewer,
    Promote,
    SwapNext,
    SwapPrevious,
    WorkspaceNext,
    WorkspacePrevious,
    FocusMonitor(Direction),
    MoveToMonitor(Direction),
    Scratchpad(String),
    Reload,
    Restart,
    Exec(Vec<String>),
}

#[derive(Clone, PartialEq)]
pub struct Binding {
    pub modifiers: Modifiers,
    pub keysym: u32,
    pub action: Action,
}

impl Modifiers {
    pub fn required(&self, primary_is_alt: bool) -> Required {
        Required {
            shift: self.shift,
            ctrl: self.ctrl,
            alt: self.alt || (self.primary && primary_is_alt),
            logo: self.logo || (self.primary && !primary_is_alt),
        }
    }
}

pub fn normalize(keysym: u32) -> u32 {
    if (0x41..=0x5a).contains(&keysym) { keysym + 0x20 } else { keysym }
}

fn named(name: &str) -> Option<u32> {
    let keysym = xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS);
    let keysym = if keysym.raw() == 0 {
        xkb::keysym_from_name(name, xkb::KEYSYM_CASE_INSENSITIVE)
    } else {
        keysym
    };
    (keysym.raw() != 0).then(|| normalize(keysym.raw()))
}

fn binding(primary: bool, shift: bool, key: &str, action: Action) -> Binding {
    Binding {
        modifiers: Modifiers {
            primary,
            shift,
            ..Modifiers::default()
        },
        keysym: named(key).expect("built in key names are valid"),
        action,
    }
}

pub fn defaults() -> Vec<Binding> {
    let mut bindings = vec![
        binding(true, false, "Return", Action::Terminal),
        binding(true, false, "q", Action::Close),
        binding(true, false, "m", Action::Maximize),
        binding(true, false, "n", Action::Minimize),
        binding(true, true, "n", Action::RestoreMinimized),
        binding(true, false, "f", Action::Fullscreen),
        binding(true, true, "e", Action::Quit),
        binding(true, false, "Tab", Action::FocusNext),
        binding(true, true, "Tab", Action::FocusPrevious),
        binding(true, false, "t", Action::ToggleTiling),
        binding(true, false, "space", Action::LayoutNext),
        binding(true, true, "space", Action::ToggleFloating),
        binding(true, false, "l", Action::MasterGrow),
        binding(true, false, "h", Action::MasterShrink),
        binding(true, false, "comma", Action::MasterMore),
        binding(true, false, "period", Action::MasterFewer),
        binding(true, true, "Return", Action::Promote),
        binding(true, true, "j", Action::SwapNext),
        binding(true, true, "k", Action::SwapPrevious),
    ];
    for index in 0..WORKSPACE_KEYS {
        let key = (index + 1).to_string();
        bindings.push(binding(true, false, &key, Action::Workspace(index)));
        bindings.push(binding(true, true, &key, Action::MoveToWorkspace(index)));
    }
    bindings
}

fn parse_modifiers(tokens: &[&str]) -> Result<Modifiers, String> {
    let mut modifiers = Modifiers::default();
    for token in tokens {
        match token.to_ascii_lowercase().as_str() {
            "mod" => modifiers.primary = true,
            "super" | "logo" | "win" => modifiers.logo = true,
            "alt" => modifiers.alt = true,
            "ctrl" | "control" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            other => return Err(format!("unknown modifier {other}")),
        }
    }
    Ok(modifiers)
}

fn parse_workspace(argument: &str) -> Result<usize, String> {
    let number: usize = argument.trim().parse().map_err(|_| "the workspace is not a number".to_string())?;
    if number == 0 {
        return Err("workspaces are numbered from 1".to_string());
    }
    Ok(number - 1)
}

pub fn split_command(text: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut characters = text.chars();

    while let Some(character) = characters.next() {
        match (quote, character) {
            (Some(open), close) if close == open => quote = None,
            (Some('"'), '\\') => match characters.next() {
                Some(escaped @ ('"' | '\\' | '$' | '`')) => current.push(escaped),
                Some(other) => {
                    current.push('\\');
                    current.push(other);
                }
                None => return Err("a backslash ends the command".to_string()),
            },
            (Some(_), other) => current.push(other),
            (None, '\'' | '"') => {
                quote = Some(character);
                started = true;
            }
            (None, '\\') => match characters.next() {
                Some(escaped) => {
                    current.push(escaped);
                    started = true;
                }
                None => return Err("a backslash ends the command".to_string()),
            },
            (None, space) if space.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, other) => {
                current.push(other);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("a quote is not closed".to_string());
    }
    if started {
        words.push(current);
    }
    Ok(words)
}

pub fn parse_action(text: &str) -> Result<Option<Action>, String> {
    let text = text.trim();
    let (name, argument) = text.split_once(char::is_whitespace).unwrap_or((text, ""));

    Ok(Some(match name {
        "none" => return Ok(None),
        "terminal" => Action::Terminal,
        "close" => Action::Close,
        "maximize" => Action::Maximize,
        "minimize" => Action::Minimize,
        "restore" => Action::RestoreMinimized,
        "fullscreen" => Action::Fullscreen,
        "quit" => Action::Quit,
        "workspace-next" => Action::WorkspaceNext,
        "workspace-prev" => Action::WorkspacePrevious,
        "focus-monitor" => Action::FocusMonitor(Direction::parse(argument)?),
        "move-to-monitor" => Action::MoveToMonitor(Direction::parse(argument)?),
        "reload" => Action::Reload,
        "restart" => Action::Restart,
        "scratchpad" => {
            if argument.is_empty() {
                return Err("scratchpad needs a name".to_string());
            }
            Action::Scratchpad(argument.to_string())
        }
        "focus-next" => Action::FocusNext,
        "focus-prev" => Action::FocusPrevious,
        "toggle-tiling" => Action::ToggleTiling,
        "layout-next" => Action::LayoutNext,
        "toggle-floating" => Action::ToggleFloating,
        "master-grow" => Action::MasterGrow,
        "master-shrink" => Action::MasterShrink,
        "master-more" => Action::MasterMore,
        "master-fewer" => Action::MasterFewer,
        "promote" => Action::Promote,
        "swap-next" => Action::SwapNext,
        "swap-prev" => Action::SwapPrevious,
        "workspace" => Action::Workspace(parse_workspace(argument)?),
        "move-to-workspace" => Action::MoveToWorkspace(parse_workspace(argument)?),
        "exec" => {
            let command = split_command(argument)?;
            if command.is_empty() {
                return Err("exec needs a command".to_string());
            }
            Action::Exec(command)
        }
        other => return Err(format!("unknown action {other}")),
    }))
}

fn parse_entry(entry: &str) -> Result<(Modifiers, u32, Option<Action>), String> {
    let (combo, action) = entry.split_once('=').ok_or("expected COMBO=ACTION")?;
    let tokens: Vec<&str> = combo.split('+').map(str::trim).collect();
    let (key, modifiers) = tokens.split_last().ok_or("the combination is empty")?;

    let modifiers = parse_modifiers(modifiers)?;
    let keysym = named(key).ok_or_else(|| format!("unknown key {key}"))?;
    Ok((modifiers, keysym, parse_action(action)?))
}

pub fn parse(entries: &[String]) -> Vec<Binding> {
    let mut bindings = defaults();

    for entry in entries {
        match parse_entry(entry) {
            Ok((modifiers, keysym, action)) => {
                bindings.retain(|existing| !(existing.modifiers == modifiers && existing.keysym == keysym));
                if let Some(action) = action {
                    bindings.push(Binding {
                        modifiers,
                        keysym,
                        action,
                    });
                }
            }
            Err(error) => tracing::warn!(entry, error, "ignoring a key binding"),
        }
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(list: &[&str]) -> Vec<String> {
        list.iter().map(|entry| entry.to_string()).collect()
    }

    fn find<'a>(bindings: &'a [Binding], modifiers: Modifiers, key: &str) -> Option<&'a Binding> {
        let keysym = named(key).unwrap();
        bindings
            .iter()
            .find(|binding| binding.modifiers == modifiers && binding.keysym == keysym)
    }

    const PRIMARY: Modifiers = Modifiers {
        primary: true,
        shift: false,
        ctrl: false,
        alt: false,
        logo: false,
    };

    #[test]
    fn defaults_cover_the_documented_keys() {
        let bindings = defaults();
        assert!(find(&bindings, PRIMARY, "Return").is_some_and(|binding| binding.action == Action::Terminal));
        assert!(find(&bindings, PRIMARY, "9").is_some_and(|binding| binding.action == Action::Workspace(8)));
    }

    #[test]
    fn entries_add_replace_and_remove() {
        let bindings = parse(&entries(&[
            "Mod+b=exec firefox --new-window",
            "Mod+Return=exec foot -e htop",
            "Mod+q=none",
            "Ctrl+Alt+Delete=quit",
        ]));

        let browser = find(&bindings, PRIMARY, "b").unwrap();
        assert!(browser.action == Action::Exec(vec!["firefox".to_string(), "--new-window".to_string()]));
        assert!(find(&bindings, PRIMARY, "Return").is_some_and(|binding| matches!(binding.action, Action::Exec(_))));
        assert!(find(&bindings, PRIMARY, "q").is_none());

        let quit = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::default()
        };
        assert!(find(&bindings, quit, "Delete").is_some_and(|binding| binding.action == Action::Quit));
    }

    #[test]
    fn bad_entries_are_ignored_and_letters_are_case_insensitive() {
        let bindings = parse(&entries(&["Mod+Q=close", "Hyper+x=quit", "Mod+nosuchkey=quit", "garbage", "Mod+9=workspace 0"]));
        assert_eq!(bindings.len(), defaults().len());
        assert!(find(&bindings, PRIMARY, "q").is_some());
    }

    #[test]
    fn commands_are_split_like_a_shell_does() {
        assert_eq!(split_command("firefox --new-window").unwrap(), vec!["firefox", "--new-window"]);
        assert_eq!(
            split_command("sh -c 'echo $HOME > \"/tmp/a b\"'").unwrap(),
            vec!["sh", "-c", "echo $HOME > \"/tmp/a b\""]
        );
        assert_eq!(split_command("a\\ b \"c d\" '' e").unwrap(), vec!["a b", "c d", "", "e"]);
        assert!(split_command("a 'b").is_err());
        assert!(split_command("a\\").is_err());
    }

    #[test]
    fn the_primary_modifier_follows_the_setting() {
        let required = PRIMARY.required(true);
        assert!(required.alt && !required.logo);
        let required = PRIMARY.required(false);
        assert!(required.logo && !required.alt);
    }
}
