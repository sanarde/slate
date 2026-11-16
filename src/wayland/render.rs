// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement, RescaleRenderElement};
use smithay::backend::renderer::element::{AsRenderElements, Element, Kind};
use smithay::backend::renderer::element::surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree};
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::desktop::{LayerSurface, layer_map_for_output};
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::output::Output;
use smithay::render_elements;
use smithay::utils::{IsAlive, Logical, Physical, Point, Rectangle, Scale};
use smithay::wayland::shell::wlr_layer::Layer;
use smithay::wayland::compositor::with_states;

use super::animation::{self, Anchor, Effect};
use super::clip::{self, ClippedSurface};
use super::decoration;
use super::shadow;
use super::tiling;
use super::shell;
use super::state::Slate;

pub const CLEAR_COLOR: [f32; 4] = [0.1, 0.1, 0.1, 1.0];

render_elements! {
    pub OutputElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Clipped=ClippedSurface,
    AnimClipped=RelocateRenderElement<RescaleRenderElement<ClippedSurface>>,
    Solid=SolidColorRenderElement,
    AnimSurface=RelocateRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>>,
    AnimSolid=RelocateRenderElement<RescaleRenderElement<SolidColorRenderElement>>,
    AnimMemory=RelocateRenderElement<RescaleRenderElement<MemoryRenderBufferRenderElement<GlesRenderer>>>,
    Memory=MemoryRenderBufferRenderElement<GlesRenderer>,
    Texture=TextureRenderElement<GlesTexture>,
    AnimTexture=RelocateRenderElement<RescaleRenderElement<TextureRenderElement<GlesTexture>>>,
}

pub fn output_elements(
    state: &mut Slate,
    renderer: &mut GlesRenderer,
    output: &Output,
    with_cursor: bool,
) -> Vec<OutputElement> {
    animation::prune(state);
    super::blur::recycle(state);
    super::closing::capture(state, renderer, output);
    let mut elements = Vec::new();

    if let Some(output_geometry) = state.space.output_geometry(output) {
        let scale = output.current_scale().fractional_scale();
        let pointer = state.pointer().current_location();
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

    super::switcher::elements(state, renderer, output, &mut elements);
    super::closing::elements(state, output, &mut elements);
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

    let mut marks: Vec<(usize, Rectangle<i32, Logical>)> = Vec::new();
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

        let content = state.space.element_geometry(window);
        let frame = decoration::frame_rect(state, window).or(content);
        let effect = frame.and_then(|frame| animation::effect_for(state, window, frame));
        let applied = shell::rule_of(window);
        let alpha = effect.as_ref().map_or(1.0, |effect| effect.alpha) * applied.opacity.unwrap_or(1.0);

        let is_focused = focused.as_ref() == Some(window);
        let group = window_group(state, renderer, output, window, output_geometry.loc, scale, alpha, frame, is_focused);

        let blurred = applied.blur && effect.is_none() && !shell::flags(window).1;
        match (effect, frame) {
            (Some(effect), Some(frame)) => elements.extend(wrap(group, &effect, frame, output_geometry.loc, scale)),
            _ => elements.extend(group),
        }
        if blurred && let Some(content) = content {
            marks.push((elements.len(), content));
        }
    }

    layer_elements(renderer, output, &lower, scale, elements);

    for (index, rect) in marks.into_iter().rev() {
        if let Some(blur) = super::blur::element(state, renderer, output, &elements[index..], rect) {
            elements.insert(index, blur);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn window_group(
    state: &Slate,
    renderer: &mut GlesRenderer,
    output: &Output,
    window: &smithay::desktop::Window,
    origin: Point<i32, Logical>,
    scale: f64,
    alpha: f32,
    frame: Option<Rectangle<i32, Logical>>,
    is_focused: bool,
) -> Vec<OutputElement> {
    let mut group: Vec<OutputElement> = Vec::new();
    if let Some(location) = state.space.element_location(window) {
        let render_location = location - window.geometry().loc - origin;
        let surfaces: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = window.render_elements(
            renderer,
            render_location.to_physical_precise_round(scale),
            Scale::from(scale),
            alpha,
        );
        group.extend(content_elements(state, renderer, output, window, surfaces, is_focused, scale));
    }
    group.extend(decoration::elements(state, renderer, window, origin, scale, is_focused, alpha));

    let (maximized, fullscreen) = shell::flags(window);
    let shadow_wanted = !shell::rule_of(window).no_shadow
        && decoration::framed(state, window)
        && !maximized
        && !fullscreen
        && (state.settings.effects.shadow_tiled || !tiling::is_tiled(window));
    if shadow_wanted && let Some(frame) = frame {
        let radius = state.settings.decoration.corner_radius as i32;
        group.extend(shadow::elements(
            state,
            renderer,
            frame,
            radius,
            if decoration::rounding(state, maximized).is_some() { radius } else { 0 },
            is_focused,
            origin,
            scale,
            alpha,
        ));
    }
    group
}

fn content_elements(
    state: &Slate,
    renderer: &mut GlesRenderer,
    output: &Output,
    window: &smithay::desktop::Window,
    surfaces: Vec<WaylandSurfaceRenderElement<GlesRenderer>>,
    focused: bool,
    scale: f64,
) -> Vec<OutputElement> {
    let effects = &state.settings.effects;
    let (maximized, fullscreen) = shell::flags(window);
    let insets = decoration::insets(state, window);
    let rounding = decoration::rounding(state, maximized).filter(|_| !fullscreen && decoration::framed(state, window));
    let rounded = rounding.is_some();
    let physical = |value: f32| value * scale as f32;
    let radii = match rounding {
        Some((_, inner)) => {
            let top = if insets.top > 0 { 0.0 } else { inner };
            [physical(top), physical(top), physical(inner), physical(inner)]
        }
        None => [0.0; 4],
    };
    let dim = if focused { 0.0 } else { effects.dim_inactive };

    let program = if rounded || dim > 0.0 { clip::program(state, renderer) } else { None };
    let Some(program) = program else {
        return surfaces.into_iter().map(OutputElement::Surface).collect();
    };

    let style = radii.iter().fold(u64::from(dim.to_bits()), |hash, radius| hash.rotate_left(7) ^ u64::from(radius.to_bits()));
    let key = (shell::window_id(window), output.name());
    let changed = state.clip_styles.borrow_mut().insert(key, style) != Some(style);

    let expected: smithay::utils::Size<i32, Physical> = window.geometry().size.to_physical_precise_round(scale);
    surfaces
        .into_iter()
        .map(|element| {
            let size = Element::geometry(&element, Scale::from(scale)).size;
            let main = (size.w - expected.w).abs() <= 1 && (size.h - expected.h).abs() <= 1;
            if main {
                OutputElement::Clipped(ClippedSurface::new(element, program.clone(), radii, dim, changed))
            } else {
                OutputElement::Surface(element)
            }
        })
        .collect()
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

pub(super) fn wrap(
    group: Vec<OutputElement>,
    effect: &Effect,
    frame: Rectangle<i32, Logical>,
    origin: Point<i32, Logical>,
    scale: f64,
) -> Vec<OutputElement> {
    let anchor: Point<f64, Logical> = match effect.anchor {
        Anchor::Center => (
            (frame.loc.x - origin.x) as f64 + frame.size.w as f64 / 2.0,
            (frame.loc.y - origin.y) as f64 + frame.size.h as f64 / 2.0,
        )
            .into(),
        Anchor::TopLeft => ((frame.loc.x - origin.x) as f64, (frame.loc.y - origin.y) as f64).into(),
    };
    let anchor: Point<i32, Physical> = anchor.to_physical(scale).to_i32_round();
    let offset: Point<i32, Physical> = effect.offset.to_physical(scale).to_i32_round();
    let factor = Scale::from(effect.scale);

    group
        .into_iter()
        .map(|element| match element {
            OutputElement::Surface(element) => OutputElement::AnimSurface(RelocateRenderElement::from_element(
                RescaleRenderElement::from_element(element, anchor, factor),
                offset,
                Relocate::Relative,
            )),
            OutputElement::Solid(element) => OutputElement::AnimSolid(RelocateRenderElement::from_element(
                RescaleRenderElement::from_element(element, anchor, factor),
                offset,
                Relocate::Relative,
            )),
            OutputElement::Clipped(element) => OutputElement::AnimClipped(RelocateRenderElement::from_element(
                RescaleRenderElement::from_element(element, anchor, factor),
                offset,
                Relocate::Relative,
            )),
            OutputElement::Memory(element) => OutputElement::AnimMemory(RelocateRenderElement::from_element(
                RescaleRenderElement::from_element(element, anchor, factor),
                offset,
                Relocate::Relative,
            )),
            other => other,
        })
        .collect()
}
