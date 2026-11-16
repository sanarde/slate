// SPDX-License-Identifier: GPL-3.0-only

use x11rb::properties::WmSizeHints;
use x11rb::protocol::xproto::*;

use super::frame::Insets;
use super::{Result, Wm};
use crate::layout::{self, Area, Layout, Params, WorkspaceLayout};
use crate::x11::geometry::Rect;

const RATIO_STEP: f32 = 0.05;
const MAX_MASTERS: usize = 8;

impl Wm {
    pub(super) fn property_strings(&self, window: Window, property: Atom) -> Vec<String> {
        let reply = self
            .conn
            .get_property(false, window, property, AtomEnum::STRING, 0, 256)
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        let Some(reply) = reply else {
            return Vec::new();
        };
        reply
            .value
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect()
    }

    fn decide_floating(&self, window: Window) -> bool {
        let class = self
            .property_strings(window, AtomEnum::WM_CLASS.into())
            .get(1)
            .cloned()
            .unwrap_or_default();
        if self.settings.tiling.floating_rules.contains(&class) {
            return true;
        }

        let transient = self
            .conn
            .get_property(false, window, AtomEnum::WM_TRANSIENT_FOR, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().and_then(|mut values| values.next()))
            .is_some_and(|parent| parent != 0);
        if transient {
            return true;
        }

        let atoms = &self.atoms;
        let types = self.atoms_of(window, atoms._NET_WM_WINDOW_TYPE);
        let floating_types = [
            atoms._NET_WM_WINDOW_TYPE_DIALOG,
            atoms._NET_WM_WINDOW_TYPE_UTILITY,
            atoms._NET_WM_WINDOW_TYPE_SPLASH,
        ];
        if types.iter().any(|kind| floating_types.contains(kind)) {
            return true;
        }

        let hints = WmSizeHints::get_normal_hints(&self.conn, window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .flatten();
        matches!(
            hints.and_then(|hints| hints.min_size.zip(hints.max_size)),
            Some((min, max)) if min == max && min.0 > 0 && min.1 > 0
        )
    }

    fn eligible(&self, index: usize) -> bool {
        let client = &self.clients[index];
        !client.floating && !client.maximized && !client.fullscreen && !client.minimized && !client.hidden
    }

    fn apply_tile(&mut self, window: Window, frame: Area) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let border = self.settings.border_width as i32;
        let insets = if self.clients[index].frame.is_some() {
            self.insets_for(index)
        } else {
            Insets {
                left: border,
                right: border,
                top: border,
                bottom: border,
            }
        };
        let rect = Rect::new(
            frame.x + insets.left,
            frame.y + insets.top,
            (frame.w - insets.left - insets.right).max(1) as u32,
            (frame.h - insets.top - insets.bottom).max(1) as u32,
        );

        let client = &mut self.clients[index];
        if !client.tiled {
            client.float_rect = Some(client.rect);
            client.tiled = true;
        }
        self.set_geometry(window, rect, border as u32)
    }

    fn release(&mut self, window: Window) -> Result<()> {
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        client.tiled = false;
        let Some(rect) = client.float_rect.take() else {
            return Ok(());
        };
        let border = self.settings.border_width;
        self.set_geometry(window, rect, border)
    }

    pub(super) fn arrange(&mut self) -> Result<()> {
        for monitor in 0..self.work_areas.len() {
            self.arrange_monitor(monitor)?;
        }
        Ok(())
    }

    pub(super) fn layout_on(&self, monitor: usize, workspace: usize) -> WorkspaceLayout {
        self.workspace_layouts[monitor.min(self.workspace_layouts.len() - 1)][workspace.min(super::WORKSPACE_LIMIT - 1)]
    }

    pub(super) fn layout_mut(&mut self, monitor: usize, workspace: usize) -> &mut WorkspaceLayout {
        let monitor = monitor.min(self.workspace_layouts.len() - 1);
        &mut self.workspace_layouts[monitor][workspace.min(super::WORKSPACE_LIMIT - 1)]
    }

    fn arrange_monitor(&mut self, monitor: usize) -> Result<()> {
        let workspace = self.workspace_on(monitor);
        let current = self.layout_on(monitor, workspace);

        let order: Vec<Window> = self
            .tile_order
            .iter()
            .copied()
            .filter(|window| {
                self.clients.iter().any(|client| {
                    client.window == *window
                        && client.workspace == workspace
                        && self.monitor_index(&client.rect) == monitor
                })
            })
            .collect();

        for window in &order {
            let index = self.clients.iter().position(|client| client.window == *window).unwrap();
            if !self.eligible(index) && self.clients[index].tiled {
                self.clients[index].tiled = false;
                self.clients[index].float_rect = None;
            }
        }

        if !current.layout.tiles() {
            for window in order {
                if self.clients.iter().any(|client| client.window == window && client.tiled) {
                    self.release(window)?;
                }
            }
            return Ok(());
        }

        let tiling = &self.settings.tiling;
        let params = Params {
            gap: tiling.gap,
            outer_gap: tiling.outer_gap,
            master_ratio: current.ratio,
            master_count: current.masters,
        };

        let area = self.work_areas[monitor];
        let area = Area {
            x: area.x,
            y: area.y,
            w: area.w as i32,
            h: area.h as i32,
        };
        let group: Vec<Window> = order
            .iter()
            .copied()
            .filter(|window| {
                let index = self.clients.iter().position(|client| client.window == *window).unwrap();
                self.eligible(index)
            })
            .collect();
        let tiles = layout::arrange(current.layout, area, group.len(), &params);
        for (window, tile) in group.into_iter().zip(tiles) {
            self.apply_tile(window, tile)?;
        }
        Ok(())
    }

    pub(super) fn register_tiled(&mut self, window: Window) {
        let floating = self.decide_floating(window);
        if let Some(client) = self.clients.iter_mut().find(|client| client.window == window) {
            client.floating = floating;
        }
        if !self.tile_order.contains(&window) {
            self.tile_order.push(window);
        }
    }

    pub(super) fn forget_tiled(&mut self, window: Window) {
        self.tile_order.retain(|known| *known != window);
    }

    pub(super) fn toggle_tiling(&mut self) -> Result<()> {
        let default = self.settings.tiling.layout;
        let (monitor, workspace) = (self.active_monitor, self.workspace());
        let entry = self.layout_mut(monitor, workspace);
        entry.layout = if entry.layout.tiles() {
            Layout::Floating
        } else if default.tiles() {
            default
        } else {
            Layout::MasterStack
        };
        self.arrange()
    }

    pub(super) fn layout_next(&mut self) -> Result<()> {
        let (monitor, workspace) = (self.active_monitor, self.workspace());
        let entry = self.layout_mut(monitor, workspace);
        entry.layout = entry.layout.next();
        self.arrange()
    }

    pub(super) fn toggle_floating(&mut self, window: Window) -> Result<()> {
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        client.floating = !client.floating;
        let floating = client.floating;
        if floating {
            self.release(window)?;
        }
        self.arrange()
    }

    pub(super) fn make_floating(&mut self, window: Window) -> Result<()> {
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        if !client.tiled {
            return Ok(());
        }
        client.floating = true;
        self.release(window)?;
        self.arrange()
    }

    pub(super) fn master_ratio(&mut self, grow: bool) -> Result<()> {
        let (monitor, workspace) = (self.active_monitor, self.workspace());
        let entry = self.layout_mut(monitor, workspace);
        let step = if grow { RATIO_STEP } else { -RATIO_STEP };
        entry.ratio = (entry.ratio + step).clamp(0.1, 0.9);
        self.arrange()
    }

    pub(super) fn master_count(&mut self, more: bool) -> Result<()> {
        let (monitor, workspace) = (self.active_monitor, self.workspace());
        let entry = self.layout_mut(monitor, workspace);
        entry.masters = if more {
            (entry.masters + 1).min(MAX_MASTERS)
        } else {
            entry.masters.saturating_sub(1).max(1)
        };
        self.arrange()
    }

    fn eligible_order(&self, monitor: usize) -> Vec<Window> {
        let workspace = self.workspace_on(monitor);
        self.tile_order
            .iter()
            .copied()
            .filter(|window| {
                self.clients.iter().enumerate().any(|(index, client)| {
                    client.window == *window
                        && client.workspace == workspace
                        && self.monitor_index(&client.rect) == monitor
                        && self.eligible(index)
                })
            })
            .collect()
    }

    fn swap(&mut self, a: Window, b: Window) -> Result<()> {
        let first = self.tile_order.iter().position(|window| *window == a);
        let second = self.tile_order.iter().position(|window| *window == b);
        if let (Some(first), Some(second)) = (first, second) {
            self.tile_order.swap(first, second);
            self.arrange()?;
        }
        Ok(())
    }

    pub(super) fn promote(&mut self, window: Window) -> Result<()> {
        let siblings = self.eligible_order(self.active_monitor);
        let Some(first) = siblings.first().copied() else {
            return Ok(());
        };
        let target = if first == window { siblings.get(1).copied() } else { Some(first) };
        match target {
            Some(other) => self.swap(window, other),
            None => Ok(()),
        }
    }

    pub(super) fn swap_neighbor(&mut self, window: Window, forward: bool) -> Result<()> {
        let siblings = self.eligible_order(self.active_monitor);
        let Some(index) = siblings.iter().position(|candidate| *candidate == window) else {
            return Ok(());
        };
        let count = siblings.len();
        if count < 2 {
            return Ok(());
        }
        let other = if forward {
            siblings[(index + 1) % count]
        } else {
            siblings[(index + count - 1) % count]
        };
        self.swap(window, other)
    }

    pub(super) fn reset_layouts(&mut self) {
        let tiling = self.settings.tiling.clone();
        for monitor in &mut self.workspace_layouts {
            for entry in monitor.iter_mut() {
                *entry = WorkspaceLayout::new(&tiling);
            }
        }
    }
}
