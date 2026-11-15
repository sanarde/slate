// SPDX-License-Identifier: GPL-3.0-only

use smithay::desktop::Window;
use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::{
    self, ZwlrForeignToplevelHandleV1,
};
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::{
    self, ZwlrForeignToplevelManagerV1,
};
use smithay::reexports::wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use super::decoration;
use super::shell;
use super::state::Slate;

const VERSION: u32 = 3;
const STATE_MAXIMIZED: u32 = 0;
const STATE_MINIMIZED: u32 = 1;
const STATE_ACTIVATED: u32 = 2;
const STATE_FULLSCREEN: u32 = 3;

pub struct HandleData {
    window: Window,
}

#[derive(Clone, PartialEq)]
struct Snapshot {
    title: String,
    app_id: String,
    states: Vec<u32>,
    output: Option<Output>,
}

struct HandleRecord {
    window: Window,
    resource: ZwlrForeignToplevelHandleV1,
    sent: Snapshot,
}

struct ManagerRecord {
    resource: ZwlrForeignToplevelManagerV1,
    handles: Vec<HandleRecord>,
}

pub struct ForeignToplevelState {
    managers: Vec<ManagerRecord>,
}

impl ForeignToplevelState {
    pub fn new(display: &DisplayHandle) -> ForeignToplevelState {
        display.create_global::<Slate, ZwlrForeignToplevelManagerV1, ()>(VERSION, ());
        ForeignToplevelState { managers: Vec::new() }
    }
}

fn listed(state: &Slate) -> Vec<Window> {
    state
        .space
        .elements()
        .filter(|window| !shell::is_override_redirect(window))
        .chain(state.parked.iter().map(|(window, _)| window))
        .chain(state.minimized.iter().map(|(window, _)| window))
        .filter(|window| shell::window_id(window) != 0)
        .cloned()
        .collect()
}

fn snapshot(state: &Slate, window: &Window) -> Snapshot {
    let (maximized, fullscreen) = shell::flags(window);
    let mut states = Vec::new();
    if maximized {
        states.push(STATE_MAXIMIZED);
    }
    if state.minimized.iter().any(|(minimized, _)| minimized == window) {
        states.push(STATE_MINIMIZED);
    }
    if shell::focused_window(state).as_ref() == Some(window) {
        states.push(STATE_ACTIVATED);
    }
    if fullscreen {
        states.push(STATE_FULLSCREEN);
    }

    Snapshot {
        title: decoration::title(window),
        app_id: decoration::app_id(window),
        states,
        output: shell::output_for_window(state, window)
            .or_else(|| state.outputs.first().map(|record| record.output.clone())),
    }
}

fn send_changes(client: &Client, handle: &ZwlrForeignToplevelHandleV1, old: Option<&Snapshot>, new: &Snapshot) {
    if old.is_none_or(|old| old.title != new.title) {
        handle.title(new.title.clone());
    }
    if old.is_none_or(|old| old.app_id != new.app_id) {
        handle.app_id(new.app_id.clone());
    }
    if old.is_none_or(|old| old.output != new.output) {
        if let Some(previous) = old.and_then(|old| old.output.as_ref()) {
            for output in previous.client_outputs(client) {
                handle.output_leave(&output);
            }
        }
        if let Some(current) = &new.output {
            for output in current.client_outputs(client) {
                handle.output_enter(&output);
            }
        }
    }
    if old.is_none_or(|old| old.states != new.states) {
        let bytes: Vec<u8> = new.states.iter().flat_map(|value| value.to_ne_bytes()).collect();
        handle.state(bytes);
    }
    handle.done();
}

fn create_handle(
    display: &DisplayHandle,
    client: &Client,
    manager: &ZwlrForeignToplevelManagerV1,
    window: &Window,
    new: Snapshot,
) -> Option<HandleRecord> {
    let handle = client
        .create_resource::<ZwlrForeignToplevelHandleV1, HandleData, Slate>(
            display,
            manager.version(),
            HandleData {
                window: window.clone(),
            },
        )
        .ok()?;
    manager.toplevel(&handle);
    send_changes(client, &handle, None, &new);
    Some(HandleRecord {
        window: window.clone(),
        resource: handle,
        sent: new,
    })
}

pub fn refresh(state: &mut Slate) {
    if state.foreign_toplevel.managers.is_empty() {
        return;
    }

    let windows = listed(state);
    let snapshots: Vec<(Window, Snapshot)> = windows
        .iter()
        .map(|window| (window.clone(), snapshot(state, window)))
        .collect();
    let display = state.display_handle.clone();

    state.foreign_toplevel.managers.retain(|manager| manager.resource.is_alive());
    for manager in &mut state.foreign_toplevel.managers {
        let Some(client) = manager.resource.client() else {
            continue;
        };

        manager.handles.retain(|record| {
            let keep = record.resource.is_alive() && windows.contains(&record.window);
            if !keep && record.resource.is_alive() {
                record.resource.closed();
            }
            keep
        });

        for (window, new) in &snapshots {
            match manager.handles.iter_mut().find(|record| &record.window == window) {
                Some(record) => {
                    if record.sent != *new {
                        send_changes(&client, &record.resource, Some(&record.sent), new);
                        record.sent = new.clone();
                    }
                }
                None => {
                    if let Some(record) = create_handle(&display, &client, &manager.resource, window, new.clone()) {
                        manager.handles.push(record);
                    }
                }
            }
        }
    }
}

impl GlobalDispatch<ZwlrForeignToplevelManagerV1, ()> for Slate {
    fn bind(
        state: &mut Slate,
        display: &DisplayHandle,
        client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Slate>,
    ) {
        let manager = data_init.init(resource, ());
        let mut record = ManagerRecord {
            resource: manager.clone(),
            handles: Vec::new(),
        };

        for window in listed(state) {
            let new = snapshot(state, &window);
            if let Some(handle) = create_handle(display, client, &manager, &window, new) {
                record.handles.push(handle);
            }
        }
        state.foreign_toplevel.managers.push(record);
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for Slate {
    fn request(
        state: &mut Slate,
        _client: &Client,
        manager: &ZwlrForeignToplevelManagerV1,
        request: zwlr_foreign_toplevel_manager_v1::Request,
        _data: &(),
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, Slate>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Request::Stop = request {
            manager.finished();
            state
                .foreign_toplevel
                .managers
                .retain(|record| record.resource != *manager);
        }
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, HandleData> for Slate {
    fn request(
        state: &mut Slate,
        _client: &Client,
        _handle: &ZwlrForeignToplevelHandleV1,
        request: zwlr_foreign_toplevel_handle_v1::Request,
        data: &HandleData,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, Slate>,
    ) {
        let window = &data.window;
        match request {
            zwlr_foreign_toplevel_handle_v1::Request::SetMaximized => shell::set_maximized(state, window, true),
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMaximized => shell::set_maximized(state, window, false),
            zwlr_foreign_toplevel_handle_v1::Request::SetMinimized => shell::minimize_window(state, window),
            zwlr_foreign_toplevel_handle_v1::Request::UnsetMinimized => shell::restore_window(state, window),
            zwlr_foreign_toplevel_handle_v1::Request::Activate { .. } => shell::activate_window(state, window),
            zwlr_foreign_toplevel_handle_v1::Request::Close => shell::close_window(window),
            zwlr_foreign_toplevel_handle_v1::Request::SetFullscreen { output: _ } => {
                shell::set_fullscreen(state, window, true);
            }
            zwlr_foreign_toplevel_handle_v1::Request::UnsetFullscreen => shell::set_fullscreen(state, window, false),
            _ => {}
        }
    }
}
