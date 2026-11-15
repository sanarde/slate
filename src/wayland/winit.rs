// SPDX-License-Identifier: GPL-3.0-only

use std::error::Error;

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::EventLoop;
use smithay::utils::Transform;

use super::input;
use super::output_management;
use super::outputs;
use super::render::{self, CLEAR_COLOR};
use super::state::Slate;

const REFRESH_MHZ: i32 = 60_000;

pub fn init(event_loop: &EventLoop<'static, Slate>, state: &mut Slate) -> Result<(), Box<dyn Error>> {
    let (mut backend, source) = winit::init::<GlesRenderer>()?;
    backend.window().set_cursor_visible(false);

    let mode = Mode {
        size: backend.window_size(),
        refresh: REFRESH_MHZ,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Slate".to_string(),
            model: "Winit".to_string(),
        },
    );
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);
    outputs::register(state, &output, (0, 0).into());

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop.handle().insert_source(source, move |event, _, state| match event {
        WinitEvent::Resized { size, .. } => {
            let mode = Mode {
                size,
                refresh: REFRESH_MHZ,
            };
            output.change_current_state(Some(mode), None, None, None);
            output.set_preferred(mode);
            smithay::desktop::layer_map_for_output(&output).arrange();
            output_management::refresh(state);
        }
        WinitEvent::Redraw => {
            if let Err(error) = redraw(&mut backend, &output, &mut damage_tracker, state) {
                tracing::error!(%error, "failed to render frame");
            }
            backend.window().request_redraw();
        }
        WinitEvent::Input(event) => input::process(state, event),
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        WinitEvent::Focus(_) => {}
    })?;

    Ok(())
}

fn redraw(
    backend: &mut WinitGraphicsBackend<GlesRenderer>,
    output: &Output,
    damage_tracker: &mut OutputDamageTracker,
    state: &mut Slate,
) -> Result<(), Box<dyn Error>> {
    let age = backend.buffer_age().unwrap_or(0);

    let damage = {
        let (renderer, mut framebuffer) = backend.bind()?;
        let elements = render::output_elements(state, renderer, output);
        let result = damage_tracker.render_output(renderer, &mut framebuffer, age, &elements, CLEAR_COLOR)?;
        result.damage.cloned()
    };

    if let Some(damage) = damage {
        backend.submit(Some(&damage))?;
    }

    state.send_frames(output);
    Ok(())
}
