// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::cfgd::{Cfgd, NotifyFd};
use crate::conf::{self, Layer};
use crate::config::{self, DEFAULTS, LIST_KEYS, PREFIX, Settings, Values};
use crate::scheme::{self, Roles, Seurat};
use crate::themes;

const MAX_VARIABLE_DEPTH: usize = 8;

type CfgdLayer = (Vec<(String, String)>, HashMap<String, Vec<String>>);

#[derive(Clone, Default)]
pub struct Options {
    pub config_path: Option<PathBuf>,
    pub theme: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Origin {
    Default,
    Theme,
    Cfgd,
    File,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Origin::Default => "default",
            Origin::Theme => "theme",
            Origin::Cfgd => "cfgd",
            Origin::File => "file",
        }
    }
}

pub struct Resolved {
    pub values: Values,
    pub origins: HashMap<String, Origin>,
    pub warnings: Vec<String>,
    pub theme: String,
    pub config_path: PathBuf,
    pub cfgd: bool,
    pub seurat: bool,
}

pub struct ConfigSource {
    options: Options,
    cfgd: Option<Cfgd>,
    seurat: Seurat,
    watched: Vec<(PathBuf, Option<SystemTime>)>,
}

fn modified(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|metadata| metadata.modified()).ok()
}

pub fn default_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SLATE_CONFIG").filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    themes::config_home().join("slate/slate.conf")
}

fn resolve_expression(text: &str, vars: &HashMap<String, String>, roles: &Roles, depth: usize) -> Result<String, String> {
    let text = text.trim();
    if depth > MAX_VARIABLE_DEPTH {
        return Err("variables refer to each other in a loop".to_string());
    }

    if let Some(name) = text.strip_prefix('$') {
        let value = vars.get(name).ok_or_else(|| format!("unknown variable ${name}"))?;
        return resolve_expression(value, vars, roles, depth + 1);
    }

    if let Some(expression) = text.strip_prefix('@') {
        let (role, alpha) = match expression.split_once('/') {
            Some((role, alpha)) => (role.trim(), Some(alpha.trim())),
            None => (expression.trim(), None),
        };
        let color = roles.get(role).ok_or_else(|| format!("unknown color role @{role}"))?;

        let mut resolved = format!("#{color:06x}");
        if let Some(alpha) = alpha {
            let percent: f32 = alpha.parse().map_err(|_| format!("the alpha {alpha} is not a number"))?;
            if !(0.0..=100.0).contains(&percent) {
                return Err(format!("the alpha {alpha} is not between 0 and 100"));
            }
            resolved.push_str(&format!("{:02x}", (percent * 2.55).round() as u32));
        }
        return Ok(resolved);
    }
    Ok(text.to_string())
}

impl ConfigSource {
    pub fn new(options: Options) -> ConfigSource {
        let mut cfgd = Cfgd::open();
        if let Some(cfgd) = cfgd.as_mut()
            && !cfgd.subscribe(PREFIX)
        {
            tracing::warn!("failed to subscribe to cfgd changes");
        }

        let mut seurat = Seurat::new();
        seurat.poll();
        ConfigSource {
            options,
            cfgd,
            seurat,
            watched: Vec::new(),
        }
    }

    pub fn notify_fd(&self) -> Option<NotifyFd> {
        self.cfgd.as_ref().and_then(Cfgd::notify_fd)
    }

    pub fn changed(&mut self) -> bool {
        let mut changed = false;
        if let Some(cfgd) = self.cfgd.as_mut() {
            changed |= cfgd.dispatch();
        }
        changed |= self.seurat.poll();
        changed |= self.watched.iter().any(|(path, seen)| modified(path) != *seen);
        changed
    }

    pub fn settings(&mut self) -> Settings {
        let resolved = self.load();
        for warning in &resolved.warnings {
            tracing::warn!("{warning}");
        }
        Settings::from_values(&resolved.values)
    }

    fn cfgd_layer(&mut self) -> CfgdLayer {
        let mut scalars = Vec::new();
        let mut lists = HashMap::new();
        let Some(cfgd) = self.cfgd.as_mut() else {
            return (scalars, lists);
        };

        for (key, default) in DEFAULTS {
            if let Some(text) = cfgd.get_text(&format!("{PREFIX}{key}"))
                && text != *default
            {
                scalars.push((key.to_string(), text));
            }
        }
        for key in LIST_KEYS {
            if let Some(list) = cfgd.get_strv(&format!("{PREFIX}{key}"))
                && !list.is_empty()
            {
                lists.insert(key.to_string(), list);
            }
        }
        (scalars, lists)
    }

    fn roles(&self, scheme_setting: &str) -> Roles {
        let dark = scheme_setting != "light";
        match self.seurat.scheme() {
            Some(scheme) => {
                let mut roles = scheme::baseline(scheme.dark);
                roles.extend(scheme.roles.iter().map(|(name, value)| (name.clone(), *value)));
                roles
            }
            None => scheme::baseline(dark),
        }
    }

    pub fn load(&mut self) -> Resolved {
        let mut warnings = Vec::new();
        let config_path = self.options.config_path.clone().unwrap_or_else(default_path);

        let mut file = Layer::default();
        conf::read_path(&mut file, &config_path, 0);
        warnings.append(&mut file.warnings);

        let (cfgd_scalars, cfgd_lists) = self.cfgd_layer();
        let cfgd_theme = cfgd_scalars
            .iter()
            .find(|(key, _)| key == "theme")
            .map(|(_, value)| value.clone());

        let theme = self
            .options
            .theme
            .clone()
            .or_else(|| file.scalar("theme").map(String::from))
            .or(cfgd_theme)
            .unwrap_or_else(|| config::default_of("theme").to_string());

        let mut theme_layer = Layer::default();
        if theme != "none" {
            match themes::find(&theme) {
                Some((text, path)) => {
                    if let Some(path) = &path {
                        theme_layer.files.push(path.clone());
                    }
                    conf::read_text(
                        &mut theme_layer,
                        &text,
                        &format!("theme {theme}"),
                        path.as_deref().and_then(|path| path.parent()),
                        1,
                    );
                }
                None => warnings.push(format!("theme {theme} was not found")),
            }
        }
        warnings.append(&mut theme_layer.warnings);

        let mut vars = theme_layer.vars.clone();
        vars.extend(file.vars.clone());

        let mut merged: BTreeMap<String, (String, Origin)> = BTreeMap::new();
        for (key, value) in &theme_layer.scalars {
            merged.insert(key.clone(), (value.clone(), Origin::Theme));
        }
        for (key, value) in &cfgd_scalars {
            merged.insert(key.clone(), (value.clone(), Origin::Cfgd));
        }
        for (key, value) in &file.scalars {
            merged.insert(key.clone(), (value.clone(), Origin::File));
        }

        let scheme_setting = merged
            .get("color-scheme")
            .map(|(value, _)| value.clone())
            .unwrap_or_else(|| config::default_of("color-scheme").to_string());
        let roles = self.roles(&scheme_setting);

        let mut values = Values::default();
        let mut origins = HashMap::new();
        for (key, (text, origin)) in merged {
            match resolve_expression(&text, &vars, &roles, 0) {
                Ok(resolved) => {
                    values.scalars.insert(key.clone(), resolved);
                    origins.insert(key, origin);
                }
                Err(error) => warnings.push(format!("{key}: {error}")),
            }
        }

        for key in LIST_KEYS {
            let entries: Vec<String> = [theme_layer.lists.get(key), cfgd_lists.get(key), file.lists.get(key)]
                .into_iter()
                .flatten()
                .flatten()
                .cloned()
                .collect();
            if !entries.is_empty() {
                values.lists.insert(key.to_string(), entries);
            }
        }

        let mut files = file.files.clone();
        files.extend(theme_layer.files.iter().cloned());
        files.sort();
        files.dedup();
        self.watched = files.into_iter().map(|path| {
            let seen = modified(&path);
            (path, seen)
        }).collect();

        Resolved {
            values,
            origins,
            warnings,
            theme,
            config_path,
            cfgd: self.cfgd.is_some(),
            seurat: self.seurat.scheme().is_some(),
        }
    }
}

pub fn dump(resolved: &Resolved) -> String {
    let mut output = format!(
        "# theme {}, config file {}, cfgd {}, seurat {}\n",
        resolved.theme,
        resolved.config_path.display(),
        if resolved.cfgd { "connected" } else { "not available" },
        if resolved.seurat { "connected" } else { "not running" },
    );

    for (key, default) in DEFAULTS {
        let (value, origin) = match resolved.values.scalars.get(*key) {
            Some(value) => (value.as_str(), resolved.origins.get(*key).copied().unwrap_or(Origin::Default)),
            None => (*default, Origin::Default),
        };
        output.push_str(&format!("{key} = {value}  # {}\n", origin.name()));
    }
    for (key, name) in [("bindings", "bind"), ("decoration-rules", "rule")] {
        for entry in resolved.values.lists.get(key).map(Vec::as_slice).unwrap_or_default() {
            output.push_str(&format!("{name} = {entry}\n"));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles() -> Roles {
        scheme::baseline(true)
    }

    #[test]
    fn variables_and_roles_resolve_to_colors() {
        let mut vars = HashMap::new();
        vars.insert("accent".to_string(), "@primary".to_string());
        vars.insert("plain".to_string(), "#112233".to_string());

        assert_eq!(resolve_expression("$plain", &vars, &roles(), 0).unwrap(), "#112233");
        assert_eq!(resolve_expression("$accent", &vars, &roles(), 0).unwrap(), "#d0bcff");
        assert_eq!(resolve_expression("@on_surface/12", &vars, &roles(), 0).unwrap(), "#e6e0e91f");
        assert_eq!(resolve_expression("40", &vars, &roles(), 0).unwrap(), "40");
    }

    #[test]
    fn bad_expressions_are_reported() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), "$b".to_string());
        vars.insert("b".to_string(), "$a".to_string());

        assert!(resolve_expression("$missing", &vars, &roles(), 0).is_err());
        assert!(resolve_expression("@nosuchrole", &vars, &roles(), 0).is_err());
        assert!(resolve_expression("@primary/200", &vars, &roles(), 0).is_err());
        assert!(resolve_expression("$a", &vars, &roles(), 0).is_err());
    }

    #[test]
    fn the_file_wins_over_the_theme_and_a_user_variable_recolors_a_preset() {
        let directory = std::env::temp_dir().join(format!("slate-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("slate.conf");
        std::fs::write(
            &path,
            "theme = nord\n$accent = #ff0000\ntitlebar-height = 50\nbind = Mod+b=exec firefox\n",
        )
        .unwrap();

        let mut source = ConfigSource {
            options: Options {
                config_path: Some(path),
                theme: None,
            },
            cfgd: None,
            seurat: Seurat::new(),
            watched: Vec::new(),
        };
        let resolved = source.load();
        std::fs::remove_dir_all(&directory).unwrap();

        assert!(resolved.warnings.is_empty(), "{:?}", resolved.warnings);
        let scalars = &resolved.values.scalars;
        assert_eq!(scalars["titlebar-height"], "50");
        assert!(resolved.origins["titlebar-height"] == Origin::File);
        assert_eq!(scalars["border-color-focused"], "#ff0000");
        assert!(resolved.origins["border-color-focused"] == Origin::Theme);
        assert_eq!(scalars["corner-radius"], "6");
        assert_eq!(resolved.values.lists["bindings"], vec!["Mod+b=exec firefox".to_string()]);
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    #[test]
    fn every_built_in_theme_is_valid() {
        for (name, text) in themes::BUILTIN {
            let mut layer = Layer::default();
            conf::read_text(&mut layer, text, name, None, 1);
            assert!(layer.warnings.is_empty(), "{name}: {:?}", layer.warnings);

            for dark in [false, true] {
                let roles = scheme::baseline(dark);
                let mut values = Values::default();
                for (key, value) in &layer.scalars {
                    let resolved = resolve_expression(value, &layer.vars, &roles, 0);
                    assert!(resolved.is_ok(), "{name}: {key}: {resolved:?}");
                    values.scalars.insert(key.clone(), resolved.unwrap());
                }
                for (key, _) in &layer.scalars {
                    let text = &values.scalars[key];
                    if key.contains("color") {
                        assert!(config::Color::parse(text).is_some(), "{name}: {key} = {text}");
                    }
                }
                Settings::from_values(&values);
            }
        }
    }
}

#[cfg(test)]
mod example_tests {
    use super::*;

    #[test]
    fn the_example_is_valid_with_every_option_enabled() {
        let example = include_str!("../data/slate.conf.example");
        let enabled: String = example
            .lines()
            .map(|line| match line.strip_prefix('#') {
                Some(rest) if !rest.is_empty() && !rest.starts_with(' ') => rest,
                _ => line,
            })
            .collect::<Vec<_>>()
            .join("\n");

        let mut layer = Layer::default();
        conf::read_text(&mut layer, &enabled, "example", None, 0);
        assert!(layer.warnings.is_empty(), "{:?}", layer.warnings);

        let roles = scheme::baseline(true);
        let mut values = Values::default();
        for (key, value) in &layer.scalars {
            let resolved = resolve_expression(value, &layer.vars, &roles, 0);
            assert!(resolved.is_ok(), "{key}: {resolved:?}");
            values.scalars.insert(key.clone(), resolved.unwrap());
        }
        values.lists = layer.lists.clone();

        let settings = Settings::from_values(&values);
        assert!(settings.bindings.len() > crate::bindings::defaults().len() - 1);
    }
}
