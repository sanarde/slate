// SPDX-License-Identifier: GPL-3.0-only

use std::error::Error;

use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitEvent, WinitGraphicsBackend};
use smithay::desktop::space::render_output;
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::EventLoop;
use smithay::utils::{Rectangle, Transform};

use super::state::Slate;

const REFRESH_MHZ: i32 = 60_000;
const CLEAR_COLOR: [f32; 4] = [0.1, 0.1, 0.1, 1.0];

pub fn init(event_loop: &EventLoop<Slate>, state: &mut Slate) -> Result<(), Box<dyn Error>> {
    let (mut backend, source) = winit::init::<GlesRenderer>()?;

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
    let _global = output.create_global::<Slate>(&state.display_handle);
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop.handle().insert_source(source, move |event, _, state| match event {
        WinitEvent::Resized { size, .. } => {
            let mode = Mode {
                size,
                refresh: REFRESH_MHZ,
            };
            output.change_current_state(Some(mode), None, None, None);
        }
        WinitEvent::Redraw => {
            if let Err(error) = redraw(&mut backend, &output, &mut damage_tracker, state) {
                tracing::error!(%error, "failed to render frame");
            }
            backend.window().request_redraw();
        }
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        WinitEvent::Input(_) | WinitEvent::Focus(_) => {}
    })?;

    Ok(())
}

fn redraw(
    backend: &mut WinitGraphicsBackend<GlesRenderer>,
    output: &Output,
    damage_tracker: &mut OutputDamageTracker,
    state: &mut Slate,
) -> Result<(), Box<dyn Error>> {
    let damage = Rectangle::from_size(backend.window_size());

    {
        let (renderer, mut framebuffer) = backend.bind()?;
        render_output::<_, WaylandSurfaceRenderElement<GlesRenderer>, _, _>(
            output,
            renderer,
            &mut framebuffer,
            1.0,
            0,
            [&state.space],
            &[],
            damage_tracker,
            CLEAR_COLOR,
        )?;
    }
    backend.submit(Some(&[damage]))?;

    let elapsed = state.start_time.elapsed();
    for window in state.space.elements() {
        window.send_frame(output, elapsed, None, |_, _| Some(output.clone()));
    }

    Ok(())
}
