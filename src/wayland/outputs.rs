// SPDX-License-Identifier: GPL-3.0-only

use smithay::desktop::layer_map_for_output;
use smithay::output::{Mode, Output, Scale};
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::utils::{Logical, Point, Transform};

use super::output_management;
use super::screencopy;
use super::state::Slate;
use super::udev;

const RESCUE_OFFSET: i32 = 32;

pub struct OutputRecord {
    pub output: Output,
    pub global: Option<GlobalId>,
    pub position: Point<i32, Logical>,
}

pub struct OutputInfo {
    pub output: Output,
    pub enabled: bool,
    pub position: Point<i32, Logical>,
}

pub struct HeadConfig {
    pub output: Output,
    pub enabled: bool,
    pub mode: Option<Mode>,
    pub position: Option<Point<i32, Logical>>,
    pub transform: Option<Transform>,
    pub scale: Option<f64>,
}

pub fn infos(state: &Slate) -> Vec<OutputInfo> {
    state
        .outputs
        .iter()
        .map(|record| OutputInfo {
            output: record.output.clone(),
            enabled: record.global.is_some(),
            position: state
                .space
                .output_geometry(&record.output)
                .map(|geometry| geometry.loc)
                .unwrap_or(record.position),
        })
        .collect()
}

pub fn register(state: &mut Slate, output: &Output, position: Point<i32, Logical>) {
    let global = output.create_global::<Slate>(&state.display_handle);
    state.space.map_output(output, position);
    state.outputs.push(OutputRecord {
        output: output.clone(),
        global: Some(global),
        position,
    });
    output_management::refresh(state);
}

pub fn unregister(state: &mut Slate, output: &Output) {
    screencopy::cancel_output(state, output);
    state.space.unmap_output(output);
    for layer in layer_map_for_output(output).layers() {
        layer.layer_surface().send_close();
    }

    if let Some(index) = state.outputs.iter().position(|record| &record.output == output) {
        let record = state.outputs.remove(index);
        if let Some(global) = record.global {
            state.display_handle.disable_global::<Slate>(global);
        }
    }

    rescue_windows(state);
    output_management::refresh(state);
}

fn set_enabled(state: &mut Slate, output: &Output, enabled: bool) {
    let Some(index) = state.outputs.iter().position(|record| &record.output == output) else {
        return;
    };

    if enabled && state.outputs[index].global.is_none() {
        let position = state.outputs[index].position;
        state.outputs[index].global = Some(output.create_global::<Slate>(&state.display_handle));
        state.space.map_output(output, position);
    } else if !enabled && let Some(global) = state.outputs[index].global.take() {
        if let Some(geometry) = state.space.output_geometry(output) {
            state.outputs[index].position = geometry.loc;
        }
        state.space.unmap_output(output);
        state.display_handle.disable_global::<Slate>(global);
    }

    if let Some(udev) = state.udev.as_mut() {
        udev.set_enabled(output, enabled);
    }
    if enabled {
        udev::redraw(state, output);
    }
}

fn validate(state: &Slate, configs: &[HeadConfig]) -> Result<(), String> {
    let nested = state.udev.is_none();

    let any_enabled = state.outputs.iter().any(|record| {
        match configs.iter().find(|config| config.output == record.output) {
            Some(config) => config.enabled,
            None => record.global.is_some(),
        }
    });
    if !any_enabled {
        return Err("at least one output must stay enabled".to_string());
    }

    for config in configs {
        if !config.enabled {
            if nested {
                return Err("an output of a nested window cannot be disabled".to_string());
            }
            continue;
        }
        if let Some(scale) = config.scale
            && (scale <= 0.0 || !scale.is_finite())
        {
            return Err("the scale must be positive".to_string());
        }
        if let Some(mode) = config.mode {
            if !config.output.modes().contains(&mode) {
                return Err(format!("{} has no mode {}x{}", config.output.name(), mode.size.w, mode.size.h));
            }
            if nested && Some(mode) != config.output.current_mode() {
                return Err("the mode of a nested window cannot be changed".to_string());
            }
        }
        if nested
            && let Some(transform) = config.transform
            && transform != config.output.current_transform()
        {
            return Err("the transform of a nested window cannot be changed".to_string());
        }
    }
    Ok(())
}

pub fn apply(state: &mut Slate, configs: Vec<HeadConfig>, test: bool) -> Result<(), String> {
    validate(state, &configs)?;
    if test {
        return Ok(());
    }

    for config in configs.iter().filter(|config| config.enabled) {
        if let Some(mode) = config.mode
            && config.output.current_mode() != Some(mode)
            && let Some(udev) = state.udev.as_mut()
        {
            udev.set_mode(&config.output, mode)?;
        }

        config.output.change_current_state(
            config.mode,
            config.transform,
            config.scale.map(Scale::Fractional),
            config.position,
        );
        if let Some(position) = config.position {
            if let Some(record) = state.outputs.iter_mut().find(|record| record.output == config.output) {
                record.position = position;
            }
            if state.space.outputs().any(|output| output == &config.output) {
                state.space.map_output(&config.output, position);
            }
        }
    }

    for config in configs.iter().filter(|config| config.enabled) {
        set_enabled(state, &config.output, true);
    }
    for config in configs.iter().filter(|config| !config.enabled) {
        set_enabled(state, &config.output, false);
    }

    for record in &state.outputs {
        layer_map_for_output(&record.output).arrange();
    }
    rescue_windows(state);
    output_management::refresh(state);
    Ok(())
}

pub fn rescue_windows(state: &mut Slate) {
    let target = state
        .space
        .outputs()
        .next()
        .and_then(|output| state.space.output_geometry(output));
    let Some(target) = target else {
        return;
    };

    let stranded: Vec<_> = state
        .space
        .elements()
        .filter(|window| {
            !window
                .x11_surface()
                .is_some_and(|surface| surface.is_override_redirect())
        })
        .filter(|window| {
            let Some(geometry) = state.space.element_geometry(window) else {
                return false;
            };
            !state.space.outputs().any(|output| {
                state
                    .space
                    .output_geometry(output)
                    .is_some_and(|output_geometry| output_geometry.overlaps(geometry))
            })
        })
        .cloned()
        .collect();

    for window in stranded {
        let location = target.loc + Point::from((RESCUE_OFFSET, RESCUE_OFFSET));
        state.space.map_element(window, location, false);
    }
}
