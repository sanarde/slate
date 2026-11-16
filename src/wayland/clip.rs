// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesRenderer, GlesTexProgram, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::{Buffer, Physical, Point, Rectangle, Scale, Transform};

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
uniform vec2 size;
uniform vec4 radii;
uniform float dim;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

void main() {
    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif

    vec2 position = v_coords * size;
    bool left = position.x < size.x * 0.5;
    bool top = position.y < size.y * 0.5;
    float radius = left ? (top ? radii.x : radii.w) : (top ? radii.y : radii.z);
    vec2 q = abs(position - size * 0.5) - (size * 0.5 - vec2(radius));
    float distance = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
    float cover = clamp(0.5 - distance, 0.0, 1.0);

    color.rgb *= 1.0 - dim;
    color *= cover * alpha;

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    gl_FragColor = color;
}
"#;

pub fn program(state: &Slate, renderer: &mut GlesRenderer) -> Option<GlesTexProgram> {
    let mut cache = state.clip_program.borrow_mut();
    cache
        .get_or_insert_with(|| {
            let compiled = renderer.compile_custom_texture_shader(
                SHADER,
                &[
                    UniformName::new("size", UniformType::_2f),
                    UniformName::new("radii", UniformType::_4f),
                    UniformName::new("dim", UniformType::_1f),
                ],
            );
            match compiled {
                Ok(program) => Some(program),
                Err(error) => {
                    tracing::error!(%error, "failed to compile the window clip shader");
                    None
                }
            }
        })
        .clone()
}

pub struct ClippedSurface {
    inner: WaylandSurfaceRenderElement<GlesRenderer>,
    program: GlesTexProgram,
    radii: [f32; 4],
    dim: f32,
    damaged: bool,
}

impl ClippedSurface {
    pub fn new(
        inner: WaylandSurfaceRenderElement<GlesRenderer>,
        program: GlesTexProgram,
        radii: [f32; 4],
        dim: f32,
        damaged: bool,
    ) -> ClippedSurface {
        ClippedSurface {
            inner,
            program,
            radii,
            dim,
            damaged,
        }
    }
}

impl Element for ClippedSurface {
    fn id(&self) -> &Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn location(&self, scale: Scale<f64>) -> Point<i32, Physical> {
        self.inner.location(scale)
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.inner.geometry(scale)
    }

    fn damage_since(&self, scale: Scale<f64>, commit: Option<CommitCounter>) -> DamageSet<i32, Physical> {
        if self.damaged {
            return DamageSet::from_slice(&[Rectangle::from_size(self.geometry(scale).size)]);
        }
        self.inner.damage_since(scale, commit)
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        if self.radii.iter().any(|radius| *radius > 0.0) {
            return OpaqueRegions::default();
        }
        self.inner.opaque_regions(scale)
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }
}

impl RenderElement<GlesRenderer> for ClippedSurface {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
    ) -> Result<(), GlesError> {
        frame.override_default_tex_program(
            self.program.clone(),
            vec![
                Uniform::new("size", (dst.size.w as f32, dst.size.h as f32)),
                Uniform::new("radii", (self.radii[0], self.radii[1], self.radii[2], self.radii[3])),
                Uniform::new("dim", self.dim),
            ],
        );
        let result = self.inner.draw(frame, src, dst, damage, opaque_regions);
        frame.clear_tex_program_override();
        result
    }

    fn underlying_storage(&self, _renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}
