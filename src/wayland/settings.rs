// SPDX-License-Identifier: GPL-3.0-only

use std::error::Error;
use std::time::Duration;

use smithay::input::keyboard::XkbConfig;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};

use super::cursor::Cursors;
use super::handlers::decoration::reapply;
use super::shell;
use super::tiling;
use super::state::{ModKey, Slate};
use crate::font::Fonts;
use crate::config::{ModKeySetting, Settings, Xkb};

const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub fn xkb_config(xkb: &Xkb) -> XkbConfig<'_> {
    XkbConfig {
        rules: &xkb.rules,
        model: &xkb.model,
        layout: &xkb.layout,
        variant: &xkb.variant,
        options: xkb.options.clone(),
    }
}

pub fn resolve_mod_key(setting: ModKeySetting, default: ModKey) -> ModKey {
    match setting {
        ModKeySetting::Auto => default,
        ModKeySetting::Super => ModKey::Logo,
        ModKeySetting::Alt => ModKey::Alt,
    }
}

pub fn watch(event_loop: &EventLoop<'static, Slate>, state: &mut Slate) -> Result<(), Box<dyn Error>> {
    let handle = event_loop.handle();

    if let Some(fd) = state.config.notify_fd() {
        handle.insert_source(Generic::new(fd, Interest::READ, Mode::Level), |_, _, state| {
            state.reload_settings();
            Ok(PostAction::Continue)
        })?;
    }

    handle.insert_source(Timer::from_duration(POLL_INTERVAL), |_, _, state| {
        state.reload_settings();
        TimeoutAction::ToDuration(POLL_INTERVAL)
    })?;
    Ok(())
}

impl Slate {
    fn reload_settings(&mut self) {
        if self.config.changed() {
            let settings = self.config.settings();
            self.apply_settings(settings);
        }
    }

    fn apply_settings(&mut self, new: Settings) {
        let old = std::mem::replace(&mut self.settings, new.clone());
        tracing::info!("settings reloaded");
        self.mod_key = resolve_mod_key(new.mod_key, self.default_mod_key);

        let keyboard = self.seat.get_keyboard().unwrap();
        if old.xkb != new.xkb
            && let Err(error) = keyboard.set_xkb_config(self, xkb_config(&new.xkb))
        {
            tracing::warn!(%error, "failed to apply the keyboard layout");
        }
        if old.repeat_delay != new.repeat_delay || old.repeat_rate != new.repeat_rate {
            keyboard.change_repeat_info(new.repeat_rate, new.repeat_delay);
        }

        if old.cursor_theme != new.cursor_theme || old.cursor_size != new.cursor_size {
            self.cursors = Cursors::new(new.cursor_theme.as_deref(), new.cursor_size);
        }

        if let Some(udev) = self.udev.as_mut() {
            udev.apply_input_settings(&new);
        }

        if old.workspace_count != new.workspace_count {
            shell::clamp_workspaces(self);
        }

        if old.decoration != new.decoration
            || old.border_width != new.border_width
            || old.border_focused != new.border_focused
            || old.border_unfocused != new.border_unfocused
        {
            if old.decoration.title_font != new.decoration.title_font {
                self.fonts = Fonts::load(&new.decoration.title_font);
            }
            self.decoration_generation += 1;
            let toplevels: Vec<_> = self.xdg_shell_state.toplevel_surfaces().to_vec();
            for toplevel in &toplevels {
                reapply(self, toplevel);
            }
            shell::refit_maximized(self);
        }

        if old.tiling != new.tiling || old.decoration != new.decoration || old.border_width != new.border_width {
            if old.tiling != new.tiling {
                tiling::reset_layouts(self);
            }
            tiling::arrange(self);
        }
    }
}
