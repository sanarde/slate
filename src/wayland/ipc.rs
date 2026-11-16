// SPDX-License-Identifier: GPL-3.0-only

use std::error::Error;

use smithay::desktop::Window;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};

use super::decoration;
use super::input;
use super::outputs;
use super::shell;
use super::state::Slate;
use super::tiling;
use crate::ipc::{
    self, Command, Handler, Ipc, OutputInfo, Snapshot, WindowInfo, WorkspaceInfo,
};
use crate::layout::Layout;

pub fn start(event_loop: &EventLoop<'static, Slate>, state: &mut Slate) -> Result<(), Box<dyn Error>> {
    let Some(ipc) = Ipc::start() else {
        return Ok(());
    };
    let Some(wake) = ipc.wake_fd() else {
        return Ok(());
    };
    state.ipc = Some(ipc);

    event_loop
        .handle()
        .insert_source(Generic::new(wake, Interest::READ, Mode::Level), |_, _, state| {
            if let Some(ipc) = state.ipc.take() {
                ipc::serve(&ipc, state);
                state.ipc = Some(ipc);
            }
            Ok(PostAction::Continue)
        })?;
    Ok(())
}

pub fn emit(state: &Slate, name: &str, fields: &[(&str, String)]) {
    if let Some(ipc) = &state.ipc {
        ipc.broadcast(ipc::event(name, fields));
    }
}

pub fn window_event(state: &Slate, name: &str, window: &Window) {
    emit(
        state,
        name,
        &[
            ("id", shell::window_id(window).to_string()),
            ("title", crate::json::quote(&decoration::title(window))),
            ("app_id", crate::json::quote(&decoration::app_id(window))),
        ],
    );
}

fn all_windows(state: &Slate) -> Vec<Window> {
    state
        .space
        .elements()
        .filter(|window| !shell::is_override_redirect(window))
        .chain(state.parked.iter().map(|(window, _)| window))
        .chain(state.minimized.iter().map(|(window, _)| window))
        .cloned()
        .collect()
}

fn window_by_id(state: &Slate, id: u64) -> Option<Window> {
    all_windows(state).into_iter().find(|window| shell::window_id(window) == id)
}

impl Handler for Slate {
    fn snapshot(&mut self) -> Snapshot {
        let focused = shell::focused_window(self);
        let windows: Vec<WindowInfo> = all_windows(self)
            .iter()
            .map(|window| {
                let (maximized, fullscreen) = shell::flags(window);
                WindowInfo {
                    id: shell::window_id(window),
                    title: decoration::title(window),
                    app_id: decoration::app_id(window),
                    workspace: shell::workspace_of(window),
                    focused: focused.as_ref() == Some(window),
                    floating: shell::is_floating(window),
                    tiled: tiling::is_tiled(window),
                    maximized,
                    fullscreen,
                    minimized: self.minimized.iter().any(|(minimized, _)| minimized == window),
                }
            })
            .collect();

        let current = shell::current_workspace(self);
        let active_name = shell::active_output(self).map(|output| output.name()).unwrap_or_default();
        let workspaces = (0..self.settings.workspace_count)
            .map(|index| WorkspaceInfo {
                index,
                layout: tiling::layout_of(self, &active_name, index).layout.name().to_string(),
                windows: windows.iter().filter(|window| window.workspace == index).count(),
                active: index == current,
            })
            .collect();

        let outputs_list = outputs::infos(self)
            .into_iter()
            .map(|info| {
                let scale = info.output.current_scale().fractional_scale();
                let size = info.output.current_mode().map(|mode| mode.size).unwrap_or_default();
                OutputInfo {
                    name: info.output.name(),
                    x: info.position.x,
                    y: info.position.y,
                    width: (size.w as f64 / scale).round() as i32,
                    height: (size.h as f64 / scale).round() as i32,
                    scale,
                    enabled: info.enabled,
                    workspace: shell::workspace_on(self, &info.output),
                }
            })
            .collect();

        Snapshot {
            workspace: current,
            focused: focused.as_ref().map(shell::window_id),
            workspaces,
            windows,
            outputs: outputs_list,
        }
    }

    fn execute(&mut self, command: Command) -> Result<(), String> {
        let window = |state: &Slate, id: u64| window_by_id(state, id).ok_or_else(|| format!("no window with id {id}"));

        match command {
            Command::Action(action) => input::run(self, action),
            Command::Focus(id) => {
                let target = window(self, id)?;
                shell::activate_window(self, &target);
            }
            Command::Close(id) => {
                let target = window(self, id)?;
                shell::close_window(self, &target);
            }
            Command::Minimize(id) => {
                let target = window(self, id)?;
                shell::minimize_window(self, &target);
            }
            Command::Restore(id) => {
                let target = window(self, id)?;
                shell::restore_window(self, &target);
            }
            Command::MoveToWorkspace(id, workspace) => {
                if workspace >= self.settings.workspace_count {
                    return Err("there is no such workspace".to_string());
                }
                let target = window(self, id)?;
                shell::move_to_workspace(self, &target, workspace);
            }
            Command::Layout(workspace, name) => {
                let layout = Layout::parse(&name).ok_or_else(|| format!("unknown layout {name}"))?;
                if workspace >= self.settings.workspace_count {
                    return Err("there is no such workspace".to_string());
                }
                let output = shell::active_output(self).map(|output| output.name()).unwrap_or_default();
                tiling::layout_mut(self, &output, workspace).layout = layout;
                tiling::arrange(self);
                emit(
                    self,
                    "layout",
                    &[
                        ("workspace", (workspace + 1).to_string()),
                        ("layout", crate::json::quote(layout.name())),
                    ],
                );
            }
            Command::Version | Command::State | Command::Windows | Command::Workspaces | Command::Outputs => {}
        }
        Ok(())
    }
}
