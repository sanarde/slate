// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent, KeyState,
    KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::desktop::{WindowSurfaceType, layer_map_for_output};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{AxisFrame, ButtonEvent, GrabStartData, MotionEvent, RelativeMotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER};
use smithay::wayland::shell::wlr_layer::Layer;

use super::focus::KeyFocus;
use super::grabs::{self, Edges};
use super::shell;
use super::state::{ModKey, Slate};
use super::udev;
use crate::spawn;

const BUTTON_LEFT: u32 = 0x110;
const BUTTON_RIGHT: u32 = 0x111;
const SCROLL_STEP: f64 = 15.0;

enum Action {
    Terminal,
    Close,
    ToggleMaximized,
    ToggleFullscreen,
    Quit,
    SwitchVt(i32),
    Workspace(usize),
    MoveToWorkspace(usize),
}

pub fn process<B: InputBackend>(state: &mut Slate, event: InputEvent<B>) {
    match event {
        InputEvent::Keyboard { event } => keyboard_key::<B>(state, event),
        InputEvent::PointerMotion { event } => pointer_motion::<B>(state, event),
        InputEvent::PointerMotionAbsolute { event } => pointer_motion_absolute::<B>(state, event),
        InputEvent::PointerButton { event } => pointer_button::<B>(state, event),
        InputEvent::PointerAxis { event } => pointer_axis::<B>(state, event),
        _ => {}
    }
}

fn mod_pressed(mod_key: ModKey, modifiers: &ModifiersState) -> bool {
    match mod_key {
        ModKey::Alt => modifiers.alt,
        ModKey::Logo => modifiers.logo,
    }
}

fn binding(mod_key: ModKey, modifiers: &ModifiersState, keysym: Keysym, unshifted: Keysym) -> Option<Action> {
    let raw = keysym.raw();
    if (Keysym::XF86_Switch_VT_1.raw()..=Keysym::XF86_Switch_VT_12.raw()).contains(&raw) {
        return Some(Action::SwitchVt((raw - Keysym::XF86_Switch_VT_1.raw() + 1) as i32));
    }
    if !mod_pressed(mod_key, modifiers) {
        return None;
    }

    if (Keysym::_1.raw()..=Keysym::_9.raw()).contains(&unshifted.raw()) {
        let index = (unshifted.raw() - Keysym::_1.raw()) as usize;
        return Some(if modifiers.shift {
            Action::MoveToWorkspace(index)
        } else {
            Action::Workspace(index)
        });
    }

    match keysym {
        Keysym::Return => Some(Action::Terminal),
        Keysym::q | Keysym::Q => Some(Action::Close),
        Keysym::m | Keysym::M => Some(Action::ToggleMaximized),
        Keysym::f | Keysym::F => Some(Action::ToggleFullscreen),
        Keysym::E if modifiers.shift => Some(Action::Quit),
        _ => None,
    }
}

fn keyboard_key<B: InputBackend>(state: &mut Slate, event: B::KeyboardKeyEvent) {
    let serial = SERIAL_COUNTER.next_serial();
    let time = Event::time_msec(&event);
    let key_state = event.state();
    let keyboard = state.seat.get_keyboard().unwrap();

    let action = keyboard.input(state, event.key_code(), key_state, serial, time, |state, modifiers, handle| {
        if key_state != KeyState::Pressed {
            return FilterResult::Forward;
        }
        let raw = handle.raw_syms().first().copied().unwrap_or(handle.modified_sym());
        match binding(state.mod_key, modifiers, handle.modified_sym(), raw) {
            Some(action) => FilterResult::Intercept(action),
            None => FilterResult::Forward,
        }
    });

    let Some(action) = action else {
        return;
    };
    match action {
        Action::Terminal => state.spawn(&spawn::terminal_command("foot")),
        Action::Close => {
            if let Some(window) = shell::focused_window(state) {
                shell::close_window(&window);
            }
        }
        Action::ToggleMaximized => {
            if let Some(window) = shell::focused_window(state) {
                shell::toggle_maximized(state, &window);
            }
        }
        Action::ToggleFullscreen => {
            if let Some(window) = shell::focused_window(state) {
                shell::toggle_fullscreen(state, &window);
            }
        }
        Action::Quit => state.loop_signal.stop(),
        Action::SwitchVt(vt) => udev::change_vt(state, vt),
        Action::Workspace(index) => shell::switch_workspace(state, index),
        Action::MoveToWorkspace(index) => {
            if let Some(window) = shell::focused_window(state) {
                shell::move_to_workspace(state, &window, index);
            }
        }
    }
}

fn outputs_bounds(state: &Slate) -> Rectangle<i32, Logical> {
    state
        .space
        .outputs()
        .filter_map(|output| state.space.output_geometry(output))
        .reduce(|bounds, geometry| bounds.merge(geometry))
        .unwrap_or_default()
}

fn clamp_to_outputs(state: &Slate, location: Point<f64, Logical>) -> Point<f64, Logical> {
    if state.space.output_under(location).next().is_some() {
        return location;
    }
    let bounds = outputs_bounds(state).to_f64();
    let max_x = bounds.loc.x + bounds.size.w - 1.0;
    let max_y = bounds.loc.y + bounds.size.h - 1.0;
    Point::from((
        location.x.clamp(bounds.loc.x, max_x.max(bounds.loc.x)),
        location.y.clamp(bounds.loc.y, max_y.max(bounds.loc.y)),
    ))
}

fn pointer_motion<B: InputBackend>(state: &mut Slate, event: B::PointerMotionEvent) {
    let pointer = state.seat.get_pointer().unwrap();
    let location = clamp_to_outputs(state, pointer.current_location() + event.delta());
    let serial = SERIAL_COUNTER.next_serial();
    let under = surface_under(state, location);

    pointer.motion(
        state,
        under.clone(),
        &MotionEvent {
            location,
            serial,
            time: event.time_msec(),
        },
    );
    pointer.relative_motion(
        state,
        under,
        &RelativeMotionEvent {
            delta: event.delta(),
            delta_unaccel: event.delta_unaccel(),
            utime: event.time(),
        },
    );
    pointer.frame(state);
}

fn pointer_motion_absolute<B: InputBackend>(state: &mut Slate, event: B::PointerMotionAbsoluteEvent) {
    let bounds = outputs_bounds(state);
    let location = event.position_transformed(bounds.size) + bounds.loc.to_f64();
    let serial = SERIAL_COUNTER.next_serial();
    let under = surface_under(state, location);
    let pointer = state.seat.get_pointer().unwrap();

    pointer.motion(
        state,
        under,
        &MotionEvent {
            location,
            serial,
            time: event.time_msec(),
        },
    );
    pointer.frame(state);
}

fn pointer_button<B: InputBackend>(state: &mut Slate, event: B::PointerButtonEvent) {
    let pointer = state.seat.get_pointer().unwrap();
    let keyboard = state.seat.get_keyboard().unwrap();
    let serial = SERIAL_COUNTER.next_serial();
    let button = event.button_code();
    let button_state = event.state();

    if button_state == ButtonState::Pressed && !pointer.is_grabbed() {
        let location = pointer.current_location();
        let window = state.space.element_under(location).map(|(window, _)| window.clone());

        if let Some(window) = window {
            shell::focus_window(state, Some(&window), serial);

            if mod_pressed(state.mod_key, &keyboard.modifier_state()) {
                let start_data = GrabStartData {
                    focus: None,
                    button,
                    location,
                };
                let seat = state.seat.clone();
                if button == BUTTON_LEFT {
                    grabs::start_move(state, &seat, window, serial, Some(start_data));
                } else if button == BUTTON_RIGHT {
                    let geometry = state.space.element_geometry(&window).unwrap();
                    let edges = Edges::nearest(geometry, location);
                    grabs::start_resize(state, &seat, window, edges, serial, Some(start_data));
                }
            }
        } else if let Some(surface) = layer_with_keyboard_under(state, location) {
            keyboard.set_focus(state, Some(KeyFocus::Wayland(surface)), serial);
        }
    }

    pointer.button(
        state,
        &ButtonEvent {
            button,
            state: button_state,
            serial,
            time: event.time_msec(),
        },
    );
    pointer.frame(state);
}

fn pointer_axis<B: InputBackend>(state: &mut Slate, event: B::PointerAxisEvent) {
    let source = event.source();
    let mut frame = AxisFrame::new(event.time_msec()).source(source);

    for axis in [Axis::Horizontal, Axis::Vertical] {
        let amount = event
            .amount(axis)
            .unwrap_or_else(|| event.amount_v120(axis).unwrap_or(0.0) * SCROLL_STEP / 120.0);
        if amount != 0.0 {
            frame = frame.value(axis, amount);
            if let Some(discrete) = event.amount_v120(axis) {
                frame = frame.v120(axis, discrete as i32);
            }
        } else if source == AxisSource::Finger {
            frame = frame.stop(axis);
        }
    }

    let pointer = state.seat.get_pointer().unwrap();
    pointer.axis(state, frame);
    pointer.frame(state);
}

fn layer_with_keyboard_under(state: &Slate, location: Point<f64, Logical>) -> Option<WlSurface> {
    let output = state.space.output_under(location).next()?;
    let output_location = state.space.output_geometry(output)?.loc.to_f64();
    let map = layer_map_for_output(output);

    [Layer::Overlay, Layer::Top, Layer::Bottom, Layer::Background]
        .into_iter()
        .filter_map(|layer| map.layer_under(layer, location - output_location))
        .find(|layer| layer.can_receive_keyboard_focus())
        .map(|layer| layer.wl_surface().clone())
}

pub fn surface_under(state: &Slate, location: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
    let output = state.space.output_under(location).next()?;
    let output_location = state.space.output_geometry(output)?.loc;
    let map = layer_map_for_output(output);
    let relative = location - output_location.to_f64();

    let layer_surface = |layer: Layer| {
        let layer_surface = map.layer_under(layer, relative)?;
        let layer_location = map.layer_geometry(layer_surface)?.loc;
        let (surface, surface_location) =
            layer_surface.surface_under(relative - layer_location.to_f64(), WindowSurfaceType::ALL)?;
        Some((surface, (surface_location + layer_location + output_location).to_f64()))
    };

    if let Some(found) = layer_surface(Layer::Overlay).or_else(|| layer_surface(Layer::Top)) {
        return Some(found);
    }

    if let Some((window, window_location)) = state.space.element_under(location)
        && let Some((surface, surface_location)) =
            window.surface_under(location - window_location.to_f64(), WindowSurfaceType::ALL)
    {
        return Some((surface, (surface_location + window_location).to_f64()));
    }

    layer_surface(Layer::Bottom).or_else(|| layer_surface(Layer::Background))
}
