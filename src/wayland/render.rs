// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::space::{SpaceRenderElements, space_render_elements};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::Output;
use smithay::render_elements;
use smithay::utils::IsAlive;
use smithay::wayland::compositor::with_states;

use super::state::Slate;

pub const CLEAR_COLOR: [f32; 4] = [0.1, 0.1, 0.1, 1.0];

render_elements! {
    pub OutputElement<=GlesRenderer>;
    Space=SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>,
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Memory=MemoryRenderBufferRenderElement<GlesRenderer>,
}

pub fn output_elements(
    state: &mut Slate,
    renderer: &mut GlesRenderer,
    output: &Output,
    with_cursor: bool,
) -> Vec<OutputElement> {
    let mut elements = Vec::new();

    if let Some(output_geometry) = state.space.output_geometry(output) {
        let scale = output.current_scale().fractional_scale();
        let pointer = state.seat.get_pointer().unwrap().current_location();
        if with_cursor && output_geometry.to_f64().contains(pointer) {
            let location = pointer - output_geometry.loc.to_f64();
            cursor_elements(state, renderer, location, scale, &mut elements);
        }
    }

    match space_render_elements(renderer, [&state.space], output, 1.0) {
        Ok(space) => elements.extend(space.into_iter().map(OutputElement::Space)),
        Err(error) => tracing::error!(?error, "failed to collect space elements"),
    }

    elements
}

fn cursor_elements(
    state: &mut Slate,
    renderer: &mut GlesRenderer,
    location: smithay::utils::Point<f64, smithay::utils::Logical>,
    scale: f64,
    elements: &mut Vec<OutputElement>,
) {
    if let CursorImageStatus::Surface(surface) = &state.cursor_status
        && !surface.alive()
    {
        state.cursor_status = CursorImageStatus::default_named();
    }

    match &state.cursor_status {
        CursorImageStatus::Hidden => {}
        CursorImageStatus::Surface(surface) => {
            let hotspot = with_states(surface, |states| {
                states
                    .data_map
                    .get::<CursorImageSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .hotspot
            });
            let position = (location - hotspot.to_f64()).to_physical(scale).to_i32_round();
            let surface_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(renderer, surface, position, scale, 1.0, Kind::Cursor);
            elements.extend(surface_elements.into_iter().map(OutputElement::Surface));
        }
        CursorImageStatus::Named(icon) => {
            let image = state.cursors.image(*icon);
            let position = (location - image.hotspot.to_f64()).to_physical(scale);
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                position,
                &image.buffer,
                None,
                None,
                None,
                Kind::Cursor,
            ) {
                Ok(element) => elements.push(OutputElement::Memory(element)),
                Err(error) => tracing::error!(%error, "failed to upload cursor image"),
            }
        }
    }
}
