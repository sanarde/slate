// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::OsString;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::Window;

use super::Wm;
use crate::ipc::{self, Command, Handler, OutputInfo, Snapshot, WindowInfo, WorkspaceInfo};
use crate::json;
use crate::layout::Layout;

impl Wm {
    pub(super) fn emit(&self, name: &str, fields: &[(&str, String)]) {
        if let Some(ipc) = &self.ipc {
            ipc.broadcast(ipc::event(name, fields));
        }
    }

    pub(super) fn window_event(&self, name: &str, window: Window) {
        let title = self.read_title(window);
        self.emit(
            name,
            &[
                ("id", window.to_string()),
                ("title", json::quote(&title)),
                ("app_id", json::quote(&self.class_of(window))),
            ],
        );
    }

    pub(super) fn spawn_env(&self) -> Vec<(&'static str, OsString)> {
        match &self.ipc {
            Some(ipc) => vec![("SLATE_SOCKET", ipc.path().as_os_str().to_owned())],
            None => Vec::new(),
        }
    }

    pub(super) fn serve_ipc(&mut self) {
        if let Some(ipc) = self.ipc.take() {
            ipc::serve(&ipc, self);
            self.ipc = Some(ipc);
        }
    }

    fn known(&self, id: u64) -> Result<Window, String> {
        let window = u32::try_from(id).map_err(|_| format!("no window with id {id}"))?;
        if self.clients.iter().any(|client| client.window == window) {
            Ok(window)
        } else {
            Err(format!("no window with id {id}"))
        }
    }
}

impl Handler for Wm {
    fn snapshot(&mut self) -> Snapshot {
        let windows: Vec<WindowInfo> = self
            .clients
            .iter()
            .map(|client| WindowInfo {
                id: client.window as u64,
                title: self.read_title(client.window),
                app_id: self.class_of(client.window),
                workspace: client.workspace,
                focused: self.focused == Some(client.window),
                floating: client.floating,
                tiled: client.tiled,
                maximized: client.maximized,
                fullscreen: client.fullscreen,
                minimized: client.minimized,
            })
            .collect();

        let workspaces = (0..self.settings.workspace_count)
            .map(|index| WorkspaceInfo {
                index,
                layout: self.layout_on(self.active_monitor, index).layout.name().to_string(),
                windows: windows.iter().filter(|window| window.workspace == index).count(),
                active: index == self.workspace(),
            })
            .collect();

        let outputs = self
            .monitors
            .iter()
            .enumerate()
            .map(|(index, monitor)| OutputInfo {
                name: format!("monitor-{}", index + 1),
                x: monitor.x,
                y: monitor.y,
                width: monitor.w as i32,
                height: monitor.h as i32,
                scale: 1.0,
                enabled: true,
                workspace: self.workspace_on(index),
            })
            .collect();

        Snapshot {
            workspace: self.workspace(),
            focused: self.focused.map(|window| window as u64),
            workspaces,
            windows,
            outputs,
        }
    }

    fn execute(&mut self, command: Command) -> Result<(), String> {
        let result = match command {
            Command::Action(action) => {
                self.active_monitor = self.pointer_monitor();
                self.run_action(action)
            }
            Command::Focus(id) => {
                let window = self.known(id)?;
                if self.clients.iter().any(|client| client.window == window && client.minimized) {
                    self.restore(window)
                } else {
                    self.focus(window)
                }
            }
            Command::Close(id) => {
                let window = self.known(id)?;
                self.close(window)
            }
            Command::Minimize(id) => {
                let window = self.known(id)?;
                self.minimize(window)
            }
            Command::Restore(id) => {
                let window = self.known(id)?;
                self.restore(window)
            }
            Command::MoveToWorkspace(id, workspace) => {
                if workspace >= self.settings.workspace_count {
                    return Err("there is no such workspace".to_string());
                }
                let window = self.known(id)?;
                self.move_to_workspace(window, workspace)
            }
            Command::Layout(workspace, name) => {
                let layout = Layout::parse(&name).ok_or_else(|| format!("unknown layout {name}"))?;
                if workspace >= self.settings.workspace_count {
                    return Err("there is no such workspace".to_string());
                }
                let monitor = self.active_monitor;
                self.layout_mut(monitor, workspace).layout = layout;
                self.emit(
                    "layout",
                    &[
                        ("workspace", (workspace + 1).to_string()),
                        ("layout", json::quote(layout.name())),
                    ],
                );
                self.arrange()
            }
            Command::Version | Command::State | Command::Windows | Command::Workspaces | Command::Outputs => Ok(()),
        };
        result.map_err(|error| error.to_string())?;
        let _ = self.conn.flush();
        Ok(())
    }
}
