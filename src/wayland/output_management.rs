// SPDX-License-Identifier: GPL-3.0-only

use std::sync::{Arc, Mutex};

use smithay::output::{Mode, Output};
use smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_configuration_head_v1::{
    self, ZwlrOutputConfigurationHeadV1,
};
use smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_configuration_v1::{
    self, ZwlrOutputConfigurationV1,
};
use smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_head_v1::{
    self, ZwlrOutputHeadV1,
};
use smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_manager_v1::{
    self, ZwlrOutputManagerV1,
};
use smithay::reexports::wayland_protocols_wlr::output_management::v1::server::zwlr_output_mode_v1::{
    self, ZwlrOutputModeV1,
};
use smithay::reexports::wayland_server::protocol::wl_output::Transform as WlTransform;
use smithay::reexports::wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum};
use smithay::utils::{Logical, Point, Transform};

use super::outputs::{self, HeadConfig, OutputInfo};
use super::state::Slate;

const VERSION: u32 = 4;
const ADAPTIVE_SYNC_SINCE: u32 = 2;

pub struct HeadData {
    output: Output,
}

pub struct ModeData {
    mode: Mode,
}

pub struct ConfigData {
    serial: u32,
    entries: Mutex<Vec<ConfigEntry>>,
}

struct ConfigEntry {
    output: Output,
    enabled: bool,
    settings: Arc<Mutex<HeadSettings>>,
}

pub struct ConfigHeadData {
    settings: Arc<Mutex<HeadSettings>>,
}

#[derive(Default)]
struct HeadSettings {
    mode: Option<Mode>,
    position: Option<Point<i32, Logical>>,
    transform: Option<Transform>,
    scale: Option<f64>,
}

#[derive(Clone, PartialEq)]
struct Snapshot {
    enabled: bool,
    mode: Option<Mode>,
    position: Point<i32, Logical>,
    transform: Transform,
    scale: f64,
}

struct HeadRecord {
    output: Output,
    resource: ZwlrOutputHeadV1,
    modes: Vec<(ZwlrOutputModeV1, Mode)>,
    sent: Snapshot,
}

struct ManagerRecord {
    resource: ZwlrOutputManagerV1,
    heads: Vec<HeadRecord>,
}

pub struct OutputManagementState {
    managers: Vec<ManagerRecord>,
    serial: u32,
}

impl OutputManagementState {
    pub fn new(display: &DisplayHandle) -> OutputManagementState {
        display.create_global::<Slate, ZwlrOutputManagerV1, ()>(VERSION, ());
        OutputManagementState {
            managers: Vec::new(),
            serial: 0,
        }
    }
}

fn snapshot(info: &OutputInfo) -> Snapshot {
    Snapshot {
        enabled: info.enabled,
        mode: info.output.current_mode(),
        position: info.position,
        transform: info.output.current_transform(),
        scale: info.output.current_scale().fractional_scale(),
    }
}

fn create_mode(
    display: &DisplayHandle,
    client: &Client,
    head: &ZwlrOutputHeadV1,
    output: &Output,
    mode: Mode,
) -> Option<ZwlrOutputModeV1> {
    let resource = client
        .create_resource::<ZwlrOutputModeV1, ModeData, Slate>(display, head.version(), ModeData { mode })
        .ok()?;
    head.mode(&resource);
    resource.size(mode.size.w, mode.size.h);
    resource.refresh(mode.refresh);
    if output.preferred_mode() == Some(mode) {
        resource.preferred();
    }
    Some(resource)
}

fn send_state(head: &ZwlrOutputHeadV1, modes: &[(ZwlrOutputModeV1, Mode)], old: Option<&Snapshot>, new: &Snapshot) {
    let was_enabled = old.is_some_and(|old| old.enabled);
    if old.is_none_or(|old| old.enabled != new.enabled) {
        head.enabled(new.enabled as i32);
    }
    if !new.enabled {
        return;
    }

    if (!was_enabled || old.is_some_and(|old| old.mode != new.mode))
        && let Some(mode) = new.mode
        && let Some((resource, _)) = modes.iter().find(|(_, known)| *known == mode)
    {
        head.current_mode(resource);
    }
    if !was_enabled || old.is_some_and(|old| old.position != new.position) {
        head.position(new.position.x, new.position.y);
    }
    if !was_enabled || old.is_some_and(|old| old.transform != new.transform) {
        head.transform(WlTransform::from(new.transform));
    }
    if !was_enabled || old.is_some_and(|old| old.scale != new.scale) {
        head.scale(new.scale);
    }
}

fn create_head(
    display: &DisplayHandle,
    client: &Client,
    manager: &ZwlrOutputManagerV1,
    info: &OutputInfo,
) -> Option<HeadRecord> {
    let head = client
        .create_resource::<ZwlrOutputHeadV1, HeadData, Slate>(
            display,
            manager.version(),
            HeadData {
                output: info.output.clone(),
            },
        )
        .ok()?;
    manager.head(&head);

    let properties = info.output.physical_properties();
    head.name(info.output.name());
    head.description(info.output.description());
    head.physical_size(properties.size.w, properties.size.h);
    if head.version() >= ADAPTIVE_SYNC_SINCE {
        head.make(properties.make);
        head.model(properties.model);
    }

    let modes: Vec<(ZwlrOutputModeV1, Mode)> = info
        .output
        .modes()
        .into_iter()
        .filter_map(|mode| create_mode(display, client, &head, &info.output, mode).map(|resource| (resource, mode)))
        .collect();

    let sent = snapshot(info);
    send_state(&head, &modes, None, &sent);
    Some(HeadRecord {
        output: info.output.clone(),
        resource: head,
        modes,
        sent,
    })
}

pub fn refresh(state: &mut Slate) {
    let infos = outputs::infos(state);
    let display = state.display_handle.clone();
    let management = &mut state.output_management;
    management.managers.retain(|manager| manager.resource.is_alive());

    let mut changed = false;
    for manager in &mut management.managers {
        let Some(client) = manager.resource.client() else {
            continue;
        };

        manager.heads.retain(|head| {
            let keep = head.resource.is_alive() && infos.iter().any(|info| info.output == head.output);
            if !keep && head.resource.is_alive() {
                head.resource.finished();
                changed = true;
            }
            keep
        });

        for info in &infos {
            let new = snapshot(info);
            let Some(head) = manager.heads.iter_mut().find(|head| head.output == info.output) else {
                if let Some(head) = create_head(&display, &client, &manager.resource, info) {
                    manager.heads.push(head);
                    changed = true;
                }
                continue;
            };
            if head.sent == new {
                continue;
            }

            if let Some(mode) = new.mode
                && !head.modes.iter().any(|(_, known)| *known == mode)
                && let Some(resource) = create_mode(&display, &client, &head.resource, &info.output, mode)
            {
                head.modes.push((resource, mode));
            }
            send_state(&head.resource, &head.modes, Some(&head.sent), &new);
            head.sent = new;
            changed = true;
        }
    }

    if changed {
        management.serial += 1;
        for manager in &management.managers {
            manager.resource.done(management.serial);
        }
    }
}

impl GlobalDispatch<ZwlrOutputManagerV1, ()> for Slate {
    fn bind(
        state: &mut Slate,
        display: &DisplayHandle,
        client: &Client,
        resource: New<ZwlrOutputManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Slate>,
    ) {
        let manager = data_init.init(resource, ());
        let mut record = ManagerRecord {
            resource: manager.clone(),
            heads: Vec::new(),
        };
        for info in &outputs::infos(state) {
            if let Some(head) = create_head(display, client, &manager, info) {
                record.heads.push(head);
            }
        }
        manager.done(state.output_management.serial);
        state.output_management.managers.push(record);
    }
}

impl Dispatch<ZwlrOutputManagerV1, ()> for Slate {
    fn request(
        state: &mut Slate,
        _client: &Client,
        manager: &ZwlrOutputManagerV1,
        request: zwlr_output_manager_v1::Request,
        _data: &(),
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, Slate>,
    ) {
        match request {
            zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } => {
                data_init.init(
                    id,
                    ConfigData {
                        serial,
                        entries: Mutex::new(Vec::new()),
                    },
                );
            }
            zwlr_output_manager_v1::Request::Stop => {
                manager.finished();
                state
                    .output_management
                    .managers
                    .retain(|record| record.resource != *manager);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputHeadV1, HeadData> for Slate {
    fn request(
        _state: &mut Slate,
        _client: &Client,
        _head: &ZwlrOutputHeadV1,
        _request: zwlr_output_head_v1::Request,
        _data: &HeadData,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, Slate>,
    ) {
    }
}

impl Dispatch<ZwlrOutputModeV1, ModeData> for Slate {
    fn request(
        _state: &mut Slate,
        _client: &Client,
        _mode: &ZwlrOutputModeV1,
        _request: zwlr_output_mode_v1::Request,
        _data: &ModeData,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, Slate>,
    ) {
    }
}

fn finish_configuration(state: &mut Slate, configuration: &ZwlrOutputConfigurationV1, data: &ConfigData, test: bool) {
    if data.serial != state.output_management.serial {
        configuration.cancelled();
        return;
    }

    let configs: Vec<HeadConfig> = data
        .entries
        .lock()
        .unwrap()
        .iter()
        .map(|entry| {
            let settings = entry.settings.lock().unwrap();
            HeadConfig {
                output: entry.output.clone(),
                enabled: entry.enabled,
                mode: settings.mode,
                position: settings.position,
                transform: settings.transform,
                scale: settings.scale,
            }
        })
        .collect();

    match outputs::apply(state, configs, test) {
        Ok(()) => configuration.succeeded(),
        Err(error) => {
            tracing::warn!(%error, "rejected an output configuration");
            configuration.failed();
        }
    }
}

impl Dispatch<ZwlrOutputConfigurationV1, ConfigData> for Slate {
    fn request(
        state: &mut Slate,
        _client: &Client,
        configuration: &ZwlrOutputConfigurationV1,
        request: zwlr_output_configuration_v1::Request,
        data: &ConfigData,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, Slate>,
    ) {
        match request {
            zwlr_output_configuration_v1::Request::EnableHead { id, head } => {
                let Some(head_data) = head.data::<HeadData>() else {
                    return;
                };
                let settings = Arc::new(Mutex::new(HeadSettings::default()));
                data_init.init(
                    id,
                    ConfigHeadData {
                        settings: settings.clone(),
                    },
                );
                data.entries.lock().unwrap().push(ConfigEntry {
                    output: head_data.output.clone(),
                    enabled: true,
                    settings,
                });
            }
            zwlr_output_configuration_v1::Request::DisableHead { head } => {
                let Some(head_data) = head.data::<HeadData>() else {
                    return;
                };
                data.entries.lock().unwrap().push(ConfigEntry {
                    output: head_data.output.clone(),
                    enabled: false,
                    settings: Arc::new(Mutex::new(HeadSettings::default())),
                });
            }
            zwlr_output_configuration_v1::Request::Apply => finish_configuration(state, configuration, data, false),
            zwlr_output_configuration_v1::Request::Test => finish_configuration(state, configuration, data, true),
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputConfigurationHeadV1, ConfigHeadData> for Slate {
    fn request(
        _state: &mut Slate,
        _client: &Client,
        _head: &ZwlrOutputConfigurationHeadV1,
        request: zwlr_output_configuration_head_v1::Request,
        data: &ConfigHeadData,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, Slate>,
    ) {
        let mut settings = data.settings.lock().unwrap();
        match request {
            zwlr_output_configuration_head_v1::Request::SetMode { mode } => {
                if let Some(mode_data) = mode.data::<ModeData>() {
                    settings.mode = Some(mode_data.mode);
                }
            }
            zwlr_output_configuration_head_v1::Request::SetCustomMode {
                width,
                height,
                refresh,
            } => {
                settings.mode = Some(Mode {
                    size: (width, height).into(),
                    refresh,
                });
            }
            zwlr_output_configuration_head_v1::Request::SetPosition { x, y } => {
                settings.position = Some((x, y).into());
            }
            zwlr_output_configuration_head_v1::Request::SetTransform {
                transform: WEnum::Value(transform),
            } => {
                settings.transform = Some(Transform::from(transform));
            }
            zwlr_output_configuration_head_v1::Request::SetScale { scale } => {
                settings.scale = Some(scale);
            }
            _ => {}
        }
    }
}
