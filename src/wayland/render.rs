// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::{AsRenderElements, Kind};
use smithay::backend::renderer::element::surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::{LayerSurface, layer_map_for_output};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::Output;
use smithay::render_elements;
use smithay::utils::{IsAlive, Rectangle, Scale};
use smithay::wayland::shell::wlr_layer::Layer;
use smithay::wayland::compositor::with_states;

use super::decoration;
use super::shell;
use super::state::Slate;

pub const CLEAR_COLOR: [f32; 4] = [0.1, 0.1, 0.1, 1.0];

render_elements! {
    pub OutputElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Solid=SolidColorRenderElement,
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

    if let Some(surface) = super::lock::surface_for(state, output) {
        let scale = output.current_scale().fractional_scale();
        let lock_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
            render_elements_from_surface_tree(renderer, &surface, (0, 0), scale, 1.0, Kind::Unspecified);
        elements.extend(lock_elements.into_iter().map(OutputElement::Surface));
        return elements;
    }
    if state.lock.is_some() {
        return elements;
    }

    scene_elements(state, renderer, output, &mut elements);
    elements
}

fn layer_elements(
    renderer: &mut GlesRenderer,
    output: &Output,
    layers: &[&LayerSurface],
    scale: f64,
    elements: &mut Vec<OutputElement>,
) {
    let map = layer_map_for_output(output);
    for layer in layers {
        let Some(geometry) = map.layer_geometry(layer) else {
            continue;
        };
        let rendered: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = layer.render_elements(
            renderer,
            geometry.loc.to_physical_precise_round(scale),
            Scale::from(scale),
            1.0,
        );
        elements.extend(rendered.into_iter().map(OutputElement::Surface));
    }
}

fn scene_elements(state: &Slate, renderer: &mut GlesRenderer, output: &Output, elements: &mut Vec<OutputElement>) {
    let Some(output_geometry) = state.space.output_geometry(output) else {
        return;
    };
    let scale = output.current_scale().fractional_scale();
    let focused = shell::focused_window(state);

    let layers: Vec<LayerSurface> = layer_map_for_output(output).layers().rev().cloned().collect();
    let (lower, upper): (Vec<&LayerSurface>, Vec<&LayerSurface>) = layers
        .iter()
        .partition(|layer| matches!(layer.layer(), Layer::Background | Layer::Bottom));

    layer_elements(renderer, output, &upper, scale, elements);

    for window in state.space.elements().rev() {
        let Some(bounds) = state.space.element_bbox(window) else {
            continue;
        };
        let framed = decoration::insets(state, window);
        let reach = Rectangle::new(
            (bounds.loc.x - framed.left, bounds.loc.y - framed.top).into(),
            (
                bounds.size.w + framed.left + framed.right,
                bounds.size.h + framed.top + framed.bottom,
            )
                .into(),
        );
        if !output_geometry.overlaps(reach) {
            continue;
        }

        if let Some(location) = state.space.element_location(window) {
            let render_location = location - window.geometry().loc - output_geometry.loc;
            let surfaces: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = window.render_elements(
                renderer,
                render_location.to_physical_precise_round(scale),
                Scale::from(scale),
                1.0,
            );
            elements.extend(surfaces.into_iter().map(OutputElement::Surface));
        }
        let is_focused = focused.as_ref() == Some(window);
        elements.extend(decoration::elements(state, renderer, window, output_geometry.loc, scale, is_focused));
    }

    layer_elements(renderer, output, &lower, scale, elements);
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
