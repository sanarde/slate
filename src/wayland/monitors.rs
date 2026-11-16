// SPDX-License-Identifier: GPL-3.0-only

use smithay::desktop::{Window, layer_map_for_output};
use smithay::input::pointer::MotionEvent;
use smithay::output::Output;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER};

use super::animation;
use super::decoration;
use super::foreign_toplevel;
use super::input;
use super::shell;
use super::state::Slate;
use super::tiling;
use crate::monitors::{self, Bounds, Direction};

fn bounds_of(rectangle: Rectangle<i32, Logical>) -> Bounds {
    (rectangle.loc.x, rectangle.loc.y, rectangle.size.w, rectangle.size.h)
}

fn zone_of(state: &Slate, output: &Output) -> Option<Rectangle<i32, Logical>> {
    let geometry = state.space.output_geometry(output)?;
    let mut zone = layer_map_for_output(output).non_exclusive_zone();
    zone.loc += geometry.loc;
    Some(zone)
}

fn outputs(state: &Slate) -> Vec<(Output, Rectangle<i32, Logical>)> {
    state
        .space
        .outputs()
        .filter_map(|output| Some((output.clone(), state.space.output_geometry(output)?)))
        .collect()
}

fn target_for(state: &Slate, current: &Output, direction: Direction) -> Option<Output> {
    let listed = outputs(state);
    let bounds: Vec<Bounds> = listed.iter().map(|(_, geometry)| bounds_of(*geometry)).collect();
    let index = listed.iter().position(|(output, _)| output == current)?;
    monitors::pick(&bounds, index, direction).map(|target| listed[target].0.clone())
}

fn pointer_output(state: &Slate) -> Option<Output> {
    let location = state.seat.get_pointer()?.current_location();
    state
        .space
        .output_under(location)
        .next()
        .or_else(|| state.space.outputs().next())
        .cloned()
}

pub fn focus_monitor(state: &mut Slate, direction: Direction) {
    let Some(current) = pointer_output(state) else {
        return;
    };
    let Some(target) = target_for(state, &current, direction) else {
        return;
    };
    let Some(zone) = zone_of(state, &target) else {
        return;
    };

    let location = (
        f64::from(zone.loc.x) + f64::from(zone.size.w) / 2.0,
        f64::from(zone.loc.y) + f64::from(zone.size.h) / 2.0,
    )
        .into();
    let pointer = state.seat.get_pointer().unwrap();
    let under = input::surface_under(state, location);
    pointer.motion(
        state,
        under,
        &MotionEvent {
            location,
            serial: SERIAL_COUNTER.next_serial(),
            time: 0,
        },
    );
    pointer.frame(state);

    let top = state
        .space
        .elements()
        .rev()
        .find(|window| !shell::is_override_redirect(window) && shell::output_for_window(state, window).as_ref() == Some(&target))
        .cloned();
    shell::focus_window(state, top.as_ref(), SERIAL_COUNTER.next_serial());
}

pub fn move_to_monitor(state: &mut Slate, window: &Window, direction: Direction) {
    let Some(source) = shell::output_for_window(state, window) else {
        return;
    };
    let Some(target) = target_for(state, &source, direction) else {
        return;
    };
    let (Some(from_zone), Some(to_zone)) = (zone_of(state, &source), zone_of(state, &target)) else {
        return;
    };

    let (maximized, fullscreen) = shell::flags(window);
    if fullscreen {
        shell::set_fullscreen(state, window, false);
    }
    if maximized {
        shell::set_maximized(state, window, false);
    }

    let Some(frame) = decoration::frame_rect(state, window).or_else(|| state.space.element_geometry(window)) else {
        return;
    };
    let insets = decoration::insets(state, window);
    let (x, y) = monitors::clamp_into(bounds_of(frame), bounds_of(from_zone), bounds_of(to_zone));
    let location = Point::from((x + insets.left, y + insets.top));

    state.space.map_element(window.clone(), location, false);
    shell::assign_workspace(window, shell::workspace_on(state, &target));
    if let Some(to) = decoration::frame_rect(state, window) {
        animation::start_geometry(state, window, frame, to);
    }
    tiling::arrange(state);
    foreign_toplevel::refresh(state);
}
