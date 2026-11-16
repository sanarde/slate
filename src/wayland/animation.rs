// SPDX-License-Identifier: GPL-3.0-only

use std::time::{Duration, Instant};

use smithay::desktop::Window;
use smithay::utils::{Logical, Point, Rectangle};

use super::state::Slate;
use crate::config::Curve;

const SLIDE_DISTANCE: f64 = 90.0;

#[derive(Clone, Copy)]
enum Kind {
    Open,
    Slide { from: Point<f64, Logical> },
    Geometry { from: Rectangle<f64, Logical> },
}

struct Animation {
    start: Instant,
    kind: Kind,
}

#[derive(Default)]
pub struct Animations {
    list: Vec<(Window, Animation)>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Anchor {
    Center,
    TopLeft,
}

pub struct Effect {
    pub alpha: f32,
    pub scale: (f64, f64),
    pub offset: Point<f64, Logical>,
    pub anchor: Anchor,
}

pub fn ease(curve: Curve, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        Curve::Linear => t,
        Curve::EaseOut => 1.0 - (1.0 - t).powi(3),
        Curve::EaseInOut => {
            if t < 0.5 {
                4.0 * t.powi(3)
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        Curve::Overshoot => {
            const OVERSHOOT: f64 = 1.70158;
            1.0 + (OVERSHOOT + 1.0) * (t - 1.0).powi(3) + OVERSHOOT * (t - 1.0).powi(2)
        }
        Curve::Standard => bezier(0.2, 0.0, 0.0, 1.0, t),
        Curve::StandardDecelerate => bezier(0.0, 0.0, 0.0, 1.0, t),
        Curve::StandardAccelerate => bezier(0.3, 0.0, 1.0, 1.0, t),
        Curve::Emphasized => bezier(0.2, 0.0, 0.0, 1.0, t),
        Curve::EmphasizedDecelerate => bezier(0.05, 0.7, 0.1, 1.0, t),
        Curve::EmphasizedAccelerate => bezier(0.3, 0.0, 0.8, 0.15, t),
    }
}

fn bezier(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    let coordinate = |a: f64, b: f64, s: f64| {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * s * a + 3.0 * inverse * s * s * b + s * s * s
    };
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if coordinate(x1, x2, middle) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    coordinate(y1, y2, (low + high) / 2.0)
}

fn duration(state: &Slate) -> Duration {
    Duration::from_millis(state.settings.effects.duration_ms)
}

fn allowed(state: &Slate, flag: bool) -> bool {
    let effects = &state.settings.effects;
    effects.animations && effects.duration_ms > 0 && flag
}

fn start(state: &mut Slate, window: &Window, kind: Kind) {
    state.animations.list.retain(|(known, _)| known != window);
    state.animations.list.push((
        window.clone(),
        Animation {
            start: Instant::now(),
            kind,
        },
    ));
}

pub fn start_open(state: &mut Slate, window: &Window) {
    if allowed(state, state.settings.effects.animate_open) {
        start(state, window, Kind::Open);
    }
}

pub fn start_slide(state: &mut Slate, window: &Window, from_right: bool) {
    if allowed(state, state.settings.effects.animate_workspace) {
        let dx = if from_right { SLIDE_DISTANCE } else { -SLIDE_DISTANCE };
        start(state, window, Kind::Slide { from: (dx, 0.0).into() });
    }
}

pub fn start_geometry(state: &mut Slate, window: &Window, from: Rectangle<i32, Logical>, to: Rectangle<i32, Logical>) {
    if from == to || !allowed(state, state.settings.effects.animate_move) {
        return;
    }
    start(state, window, Kind::Geometry { from: from.to_f64() });
}

pub fn forget(state: &mut Slate, window: &Window) {
    state.animations.list.retain(|(known, _)| known != window);
}

pub fn prune(state: &mut Slate) {
    let length = duration(state);
    state.animations.list.retain(|(_, animation)| animation.start.elapsed() < length);
}

pub fn effect_for(state: &Slate, window: &Window, target: Rectangle<i32, Logical>) -> Option<Effect> {
    let (_, animation) = state.animations.list.iter().find(|(known, _)| known == window)?;
    let length = duration(state).as_secs_f64();
    if length <= 0.0 {
        return None;
    }

    let linear = animation.start.elapsed().as_secs_f64() / length;
    let progress = ease(state.settings.effects.curve, linear);
    let to = target.to_f64();

    Some(match animation.kind {
        Kind::Open => {
            let start = state.settings.effects.open_scale as f64;
            let scale = start + (1.0 - start) * progress;
            Effect {
                alpha: linear.clamp(0.0, 1.0) as f32,
                scale: (scale, scale),
                offset: (0.0, 0.0).into(),
                anchor: Anchor::Center,
            }
        }
        Kind::Slide { from } => Effect {
            alpha: linear.clamp(0.0, 1.0) as f32,
            scale: (1.0, 1.0),
            offset: (from.x * (1.0 - progress), from.y * (1.0 - progress)).into(),
            anchor: Anchor::TopLeft,
        },
        Kind::Geometry { from } => {
            let lerp = |a: f64, b: f64| a + (b - a) * progress;
            let (x, y) = (lerp(from.loc.x, to.loc.x), lerp(from.loc.y, to.loc.y));
            let (w, h) = (lerp(from.size.w, to.size.w), lerp(from.size.h, to.size.h));
            Effect {
                alpha: 1.0,
                scale: (w / to.size.w.max(1.0), h / to.size.h.max(1.0)),
                offset: (x - to.loc.x, y - to.loc.y).into(),
                anchor: Anchor::TopLeft,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for curve in [
            Curve::Linear,
            Curve::EaseOut,
            Curve::EaseInOut,
            Curve::Overshoot,
            Curve::Standard,
            Curve::StandardDecelerate,
            Curve::StandardAccelerate,
            Curve::Emphasized,
            Curve::EmphasizedDecelerate,
            Curve::EmphasizedAccelerate,
        ] {
            assert!(ease(curve, 0.0).abs() < 1e-9, "{}", ease(curve, 0.0));
            assert!((ease(curve, 1.0) - 1.0).abs() < 1e-9);
        }
        assert!(ease(Curve::EaseOut, 0.5) > 0.5);
        assert!(ease(Curve::EaseInOut, 0.5) - 0.5 < 1e-9);
        assert!(ease(Curve::Overshoot, 0.8) > 1.0);
        assert!(ease(Curve::EmphasizedDecelerate, 0.3) > 0.6);
        assert!(ease(Curve::EmphasizedAccelerate, 0.5) < 0.2);
        assert!(ease(Curve::Standard, 0.5) > 0.8);
    }
}
