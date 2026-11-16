// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement, RescaleRenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, Offscreen};
use smithay::desktop::Window;
use smithay::output::Output;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};

use super::animation;
use super::decoration;
use super::render::{self, OutputElement};
use super::shell;
use super::state::Slate;

const REQUEST_LIFETIME: Duration = Duration::from_secs(1);
const SNAPSHOT_LIFETIME: Duration = Duration::from_secs(5);

struct Snapshot {
    window: Window,
    output: String,
    rect: Rectangle<i32, Logical>,
    frame: Rectangle<i32, Logical>,
    texture: TextureBuffer<GlesTexture>,
    taken: Instant,
}

struct Ghost {
    output: String,
    rect: Rectangle<i32, Logical>,
    frame: Rectangle<i32, Logical>,
    texture: TextureBuffer<GlesTexture>,
    start: Instant,
}

#[derive(Default)]
pub struct Closing {
    requests: Vec<(Window, Instant)>,
    snapshots: Vec<Snapshot>,
    ghosts: Vec<Ghost>,
}

impl Closing {
    pub fn active(&self) -> bool {
        !self.ghosts.is_empty() || !self.requests.is_empty()
    }
}

fn enabled(state: &Slate) -> bool {
    let effects = &state.settings.effects;
    effects.animations && effects.animate_close && effects.duration_ms > 0
}

pub fn request(state: &mut Slate, window: &Window) {
    if !enabled(state) || state.closing.requests.iter().any(|(known, _)| known == window) {
        return;
    }
    state.closing.requests.push((window.clone(), Instant::now()));
}

pub fn promote(state: &mut Slate, window: &Window) {
    let Some(index) = state.closing.snapshots.iter().position(|snapshot| &snapshot.window == window) else {
        return;
    };
    let snapshot = state.closing.snapshots.remove(index);
    if !enabled(state) {
        return;
    }
    state.closing.ghosts.push(Ghost {
        output: snapshot.output,
        rect: snapshot.rect,
        frame: snapshot.frame,
        texture: snapshot.texture,
        start: Instant::now(),
    });
}

fn duration(state: &Slate) -> Duration {
    Duration::from_millis(state.settings.effects.duration_ms)
}

pub fn prune(state: &mut Slate) {
    let length = duration(state);
    let closing = &mut state.closing;
    closing.requests.retain(|(window, at)| {
        at.elapsed() < REQUEST_LIFETIME && smithay::utils::IsAlive::alive(window)
    });
    closing.snapshots.retain(|snapshot| snapshot.taken.elapsed() < SNAPSHOT_LIFETIME);
    closing.ghosts.retain(|ghost| ghost.start.elapsed() < length);
}

fn take(state: &mut Slate, renderer: &mut GlesRenderer, output: &Output, window: &Window) -> Option<Snapshot> {
    let output_geometry = state.space.output_geometry(output)?;
    let scale = output.current_scale().fractional_scale();
    let content = state.space.element_geometry(window);
    let frame = decoration::frame_rect(state, window).or(content)?;
    let margin = state.settings.effects.shadow_size.max(0)
        + state.settings.effects.shadow_offset.0.abs().max(state.settings.effects.shadow_offset.1.abs());
    let rect = Rectangle::new(
        (frame.loc.x - margin, frame.loc.y - margin).into(),
        (frame.size.w + 2 * margin, frame.size.h + 2 * margin).into(),
    );
    if !output_geometry.overlaps(rect) {
        return None;
    }

    let size: Size<i32, Physical> = rect.size.to_physical_precise_round(scale);
    let buffer_size: Size<i32, Buffer> = (size.w, size.h).into();
    let mut texture: GlesTexture = Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, buffer_size).ok()?;

    let focused = shell::focused_window(state).as_ref() == Some(window);
    let opacity = shell::rule_of(window).opacity.unwrap_or(1.0);
    let elements = render::window_group(state, renderer, output, window, rect.loc, scale, opacity, Some(frame), focused);
    {
        let mut framebuffer = renderer.bind(&mut texture).ok()?;
        OutputDamageTracker::new(size, scale, Transform::Normal)
            .render_output(renderer, &mut framebuffer, 0, &elements, [0.0; 4])
            .ok()?;
    }

    Some(Snapshot {
        window: window.clone(),
        output: output.name(),
        rect,
        frame,
        texture: TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None),
        taken: Instant::now(),
    })
}

pub fn capture(state: &mut Slate, renderer: &mut GlesRenderer, output: &Output) {
    prune(state);
    let requests: Vec<Window> = state.closing.requests.iter().map(|(window, _)| window.clone()).collect();
    for window in requests {
        if shell::output_for_window(state, &window).as_ref() != Some(output) {
            continue;
        }
        state.closing.requests.retain(|(known, _)| known != &window);
        if let Some(snapshot) = take(state, renderer, output, &window) {
            state.closing.snapshots.retain(|known| known.window != window);
            state.closing.snapshots.push(snapshot);
        }
    }
}

pub fn elements(state: &Slate, output: &Output, elements: &mut Vec<OutputElement>) {
    let Some(output_geometry) = state.space.output_geometry(output) else {
        return;
    };
    let scale = output.current_scale().fractional_scale();
    let name = output.name();
    let length = duration(state).as_secs_f64();
    if length <= 0.0 {
        return;
    }

    for ghost in state.closing.ghosts.iter().filter(|ghost| ghost.output == name) {
        let linear = (ghost.start.elapsed().as_secs_f64() / length).clamp(0.0, 1.0);
        let progress = animation::ease(state.settings.effects.curve, linear);
        let factor = 1.0 - (1.0 - f64::from(state.settings.effects.open_scale)) * progress;

        let location: Point<f64, Physical> = (ghost.rect.loc - output_geometry.loc).to_f64().to_physical(scale);
        let element = TextureRenderElement::from_texture_buffer(
            location,
            &ghost.texture,
            Some((1.0 - linear) as f32),
            None,
            Some(ghost.rect.size),
            Kind::Unspecified,
        );
        let middle: Point<f64, Logical> = (
            (ghost.frame.loc.x - output_geometry.loc.x) as f64 + ghost.frame.size.w as f64 / 2.0,
            (ghost.frame.loc.y - output_geometry.loc.y) as f64 + ghost.frame.size.h as f64 / 2.0,
        )
            .into();
        let center: Point<i32, Physical> = middle.to_physical(scale).to_i32_round();
        elements.push(OutputElement::AnimTexture(RelocateRenderElement::from_element(
            RescaleRenderElement::from_element(element, center, Scale::from(factor)),
            (0, 0),
            Relocate::Relative,
        )));
    }
}
