// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType};
use smithay::backend::renderer::{Bind, Color32F, Frame, Offscreen, Renderer};
use smithay::output::Output;
use smithay::utils::{Buffer, Logical, Physical, Rectangle, Size, Transform};

use super::render::OutputElement;
use super::state::Slate;

const SHADER: &str = r#"#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;
uniform vec2 blur_step;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

void main() {
    vec4 color = texture2D(tex, v_coords) * 0.2270270270;
    color += texture2D(tex, v_coords + blur_step * 1.0) * 0.1945945946;
    color += texture2D(tex, v_coords - blur_step * 1.0) * 0.1945945946;
    color += texture2D(tex, v_coords + blur_step * 2.0) * 0.1216216216;
    color += texture2D(tex, v_coords - blur_step * 2.0) * 0.1216216216;
    color += texture2D(tex, v_coords + blur_step * 3.0) * 0.0540540541;
    color += texture2D(tex, v_coords - blur_step * 3.0) * 0.0540540541;
    color += texture2D(tex, v_coords + blur_step * 4.0) * 0.0162162162;
    color += texture2D(tex, v_coords - blur_step * 4.0) * 0.0162162162;

#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif

    gl_FragColor = color * alpha;
}
"#;

fn program(state: &Slate, renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    state
        .blur_program
        .borrow_mut()
        .get_or_insert_with(|| {
            match renderer.compile_custom_texture_shader(SHADER, &[UniformName::new("blur_step", UniformType::_2f)]) {
                Ok(program) => Some(program),
                Err(error) => {
                    tracing::error!(%error, "failed to compile the blur shader");
                    None
                }
            }
        })
        .clone()
}

const POOL_LIMIT: usize = 12;

#[derive(Default)]
pub struct BlurPool {
    free: Vec<(Size<i32, Physical>, GlesTexture)>,
    in_flight: Vec<(Size<i32, Physical>, GlesTexture)>,
}

impl BlurPool {
    fn keep(&mut self, entry: (Size<i32, Physical>, GlesTexture)) {
        if self.free.len() >= POOL_LIMIT {
            self.free.remove(0);
        }
        self.free.push(entry);
    }
}

pub fn recycle(state: &Slate) {
    let mut pool = state.blur_pool.borrow_mut();
    for entry in std::mem::take(&mut pool.in_flight) {
        pool.keep(entry);
    }
}

fn acquire(state: &Slate, renderer: &mut GlesRenderer, size: Size<i32, Physical>) -> Option<GlesTexture> {
    {
        let mut pool = state.blur_pool.borrow_mut();
        if let Some(index) = pool.free.iter().position(|(known, _)| *known == size) {
            return Some(pool.free.swap_remove(index).1);
        }
    }
    texture(renderer, size)
}

fn texture(renderer: &mut GlesRenderer, size: Size<i32, Physical>) -> Option<GlesTexture> {
    let size: Size<i32, Buffer> = (size.w.max(1), size.h.max(1)).into();
    Offscreen::<GlesTexture>::create_buffer(renderer, Fourcc::Abgr8888, size).ok()
}

fn pass(
    renderer: &mut GlesRenderer,
    source: &GlesTexture,
    from: Rectangle<f64, Buffer>,
    target: &mut GlesTexture,
    size: Size<i32, Physical>,
    program: &GlesTexProgram,
    step: (f32, f32),
) -> Option<()> {
    let mut framebuffer = renderer.bind(target).ok()?;
    let mut frame = renderer.render(&mut framebuffer, size, Transform::Normal).ok()?;
    let whole = Rectangle::from_size(size);
    frame.clear(Color32F::TRANSPARENT, &[whole]).ok()?;
    frame
        .render_texture_from_to(
            source,
            from,
            whole,
            &[whole],
            &[],
            Transform::Normal,
            1.0,
            Some(program),
            &[Uniform::new("blur_step", step)],
        )
        .ok()?;
    let sync = frame.finish().ok()?;
    let _ = sync.wait();
    Some(())
}

pub fn element(
    state: &Slate,
    renderer: &mut GlesRenderer,
    output: &Output,
    behind: &[OutputElement],
    rect: Rectangle<i32, Logical>,
) -> Option<OutputElement> {
    let effects = &state.settings.effects;
    if effects.blur_radius <= 0 {
        return None;
    }
    let program = program(state, renderer)?;
    let geometry = state.space.output_geometry(output)?;
    let scale = output.current_scale().fractional_scale();
    let mode = output.current_mode()?;

    let local = Rectangle::new(rect.loc - geometry.loc, rect.size);
    let physical: Rectangle<i32, Physical> = local.to_physical_precise_round(scale);
    let bounds = Rectangle::from_size(mode.size);
    let physical = physical.intersection(bounds)?;
    if physical.size.w < 2 || physical.size.h < 2 {
        return None;
    }

    let mut backdrop = acquire(state, renderer, mode.size)?;
    {
        let mut framebuffer = renderer.bind(&mut backdrop).ok()?;
        OutputDamageTracker::new(mode.size, scale, Transform::Normal)
            .render_output(renderer, &mut framebuffer, 0, behind, [0.0, 0.0, 0.0, 1.0])
            .ok()?;
    }

    let size: Size<i32, Physical> = physical.size;
    let radius = effects.blur_radius as f32;
    let from: Rectangle<f64, Buffer> = Rectangle::new(
        (f64::from(physical.loc.x), f64::from(physical.loc.y)).into(),
        (f64::from(physical.size.w), f64::from(physical.size.h)).into(),
    );

    let mut first = acquire(state, renderer, size)?;
    let mut second = acquire(state, renderer, size)?;
    let step_x = (2.0 * radius / mode.size.w as f32, 0.0);
    pass(renderer, &backdrop, from, &mut first, size, &program, step_x)?;
    let whole: Rectangle<f64, Buffer> = Rectangle::new((0.0, 0.0).into(), (f64::from(size.w), f64::from(size.h)).into());
    let step_y = (0.0, 2.0 * radius / size.h as f32);
    pass(renderer, &first, whole, &mut second, size, &program, step_y)?;
    for _ in 1..effects.blur_passes {
        pass(renderer, &second, whole, &mut first, size, &program, (2.0 * radius / size.w as f32, 0.0))?;
        pass(renderer, &first, whole, &mut second, size, &program, step_y)?;
    }

    {
        let mut pool = state.blur_pool.borrow_mut();
        pool.keep((mode.size, backdrop));
        pool.keep((size, first));
        pool.in_flight.push((size, second.clone()));
    }
    let buffer = TextureBuffer::from_texture(renderer, second, 1, Transform::Normal, None);
    let logical: Rectangle<f64, Logical> = physical.to_f64().to_logical(scale);
    let logical_size: Size<i32, Logical> = (logical.size.w.round() as i32, logical.size.h.round() as i32).into();
    Some(OutputElement::Texture(TextureRenderElement::from_texture_buffer(
        (f64::from(physical.loc.x), f64::from(physical.loc.y)),
        &buffer,
        Some(1.0),
        None,
        Some(logical_size),
        Kind::Unspecified,
    )))
}
