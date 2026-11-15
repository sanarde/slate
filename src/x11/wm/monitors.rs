// SPDX-License-Identifier: GPL-3.0-only

use x11rb::protocol::xproto::*;
use x11rb::NONE;

use super::{Result, Wm};
use crate::monitors::{self, Bounds, Direction};
use crate::x11::geometry::Rect;

fn bounds_of(rect: &Rect) -> Bounds {
    (rect.x, rect.y, rect.w as i32, rect.h as i32)
}

impl Wm {
    fn monitor_target(&self, current: usize, direction: Direction) -> Option<usize> {
        let bounds: Vec<Bounds> = self.monitors.iter().map(bounds_of).collect();
        monitors::pick(&bounds, current, direction)
    }

    pub(super) fn focus_monitor(&mut self, direction: Direction) -> Result<()> {
        let current = self.pointer_monitor();
        let Some(target) = self.monitor_target(current, direction) else {
            return Ok(());
        };
        let area = self.work_areas.get(target).copied().unwrap_or(self.monitors[target]);

        self.conn.warp_pointer(
            NONE,
            self.root,
            0,
            0,
            0,
            0,
            (area.x + (area.w / 2) as i32) as i16,
            (area.y + (area.h / 2) as i32) as i16,
        )?;

        let workspace = self.workspace;
        let top = self
            .clients
            .iter()
            .rev()
            .find(|client| client.workspace == workspace && !client.hidden && self.monitor_index(&client.rect) == target)
            .map(|client| client.window);
        match top {
            Some(window) => self.focus(window),
            None => Ok(()),
        }
    }

    pub(super) fn move_to_monitor(&mut self, window: Window, direction: Direction) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let current = self.monitor_index(&self.clients[index].rect);
        let Some(target) = self.monitor_target(current, direction) else {
            return Ok(());
        };

        if self.clients[index].fullscreen || self.clients[index].maximized {
            self.set_state(window, Some(false), Some(false))?;
        }
        let index = self.clients.iter().position(|client| client.window == window).unwrap();
        let rect = self.clients[index].rect;
        let insets = self.insets_for(index);

        let source = self.work_areas.get(current).copied().unwrap_or(self.monitors[current]);
        let destination = self.work_areas.get(target).copied().unwrap_or(self.monitors[target]);
        let frame = (
            rect.x - insets.left,
            rect.y - insets.top,
            rect.w as i32 + insets.left + insets.right,
            rect.h as i32 + insets.top + insets.bottom,
        );
        let (x, y) = monitors::clamp_into(frame, bounds_of(&source), bounds_of(&destination));
        let moved = Rect::new(x + insets.left, y + insets.top, rect.w, rect.h);

        let border = self.settings.border_width;
        self.set_geometry(window, moved, border)?;
        self.arrange()?;
        self.restack()
    }
}
