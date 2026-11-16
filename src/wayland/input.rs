// SPDX-License-Identifier: GPL-3.0-only

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, GestureBeginEvent, GestureEndEvent,
    GesturePinchUpdateEvent as _, GestureSwipeUpdateEvent as _, InputBackend, InputEvent, KeyState, KeyboardKeyEvent,
    PointerAxisEvent, PointerButtonEvent, PointerMotionEvent, TouchEvent,
};
use smithay::input::pointer as gesture;
use smithay::input::touch::{DownEvent, MotionEvent as TouchMotionEvent, UpEvent};
use smithay::desktop::{WindowSurfaceType, layer_map_for_output};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{AxisFrame, ButtonEvent, GrabStartData, MotionEvent, RelativeMotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER};
use smithay::wayland::shell::wlr_layer::Layer;

use super::decoration;
use super::focus::KeyFocus;
use super::grabs::{self, Edges};
use super::shell;
use super::switcher;
use super::state::{ModKey, Slate};
use super::tiling;
use super::udev;
use crate::bindings::{self, Action, Required};
use crate::spawn;

const BUTTON_LEFT: u32 = 0x110;
const BUTTON_RIGHT: u32 = 0x111;
const SCROLL_STEP: f64 = 15.0;

enum Pressed {
    SwitchVt(i32),
    Bound(Action),
    Switcher(SwitcherKey),
}

enum SwitcherKey {
    Step(bool),
    Move(i32, i32),
    Commit,
    Cancel,
    Ignore,
}

fn switcher_key(keysym: Keysym, modifiers: &ModifiersState) -> SwitcherKey {
    match keysym {
        Keysym::Tab => SwitcherKey::Step(!modifiers.shift),
        Keysym::ISO_Left_Tab => SwitcherKey::Step(false),
        Keysym::Escape => SwitcherKey::Cancel,
        Keysym::Return | Keysym::KP_Enter => SwitcherKey::Commit,
        Keysym::Left => SwitcherKey::Move(-1, 0),
        Keysym::Right => SwitcherKey::Move(1, 0),
        Keysym::Up => SwitcherKey::Move(0, -1),
        Keysym::Down => SwitcherKey::Move(0, 1),
        _ => SwitcherKey::Ignore,
    }
}

pub fn process<B: InputBackend>(state: &mut Slate, event: InputEvent<B>) {
    state.idle_notifier_state.notify_activity(&state.seat);
    match event {
        InputEvent::Keyboard { event } => keyboard_key::<B>(state, event),
        InputEvent::PointerMotion { event } => pointer_motion::<B>(state, event),
        InputEvent::PointerMotionAbsolute { event } => pointer_motion_absolute::<B>(state, event),
        InputEvent::PointerButton { event } => pointer_button::<B>(state, event),
        InputEvent::PointerAxis { event } => pointer_axis::<B>(state, event),
        InputEvent::GestureSwipeBegin { event } => swipe_begin::<B>(state, event),
        InputEvent::GestureSwipeUpdate { event } => swipe_update::<B>(state, event),
        InputEvent::GestureSwipeEnd { event } => swipe_end::<B>(state, event),
        InputEvent::GesturePinchBegin { event } => {
            let fingers = event.fingers();
            let (serial, time) = (SERIAL_COUNTER.next_serial(), event.time_msec());
            let pointer = state.pointer();
            pointer.gesture_pinch_begin(state, &gesture::GesturePinchBeginEvent { serial, time, fingers });
            pointer.frame(state);
        }
        InputEvent::GesturePinchUpdate { event } => {
            let pointer = state.pointer();
            pointer.gesture_pinch_update(
                state,
                &gesture::GesturePinchUpdateEvent {
                    time: event.time_msec(),
                    delta: (event.delta_x(), event.delta_y()).into(),
                    scale: event.scale(),
                    rotation: event.rotation(),
                },
            );
            pointer.frame(state);
        }
        InputEvent::GesturePinchEnd { event } => {
            let (serial, time, cancelled) = (SERIAL_COUNTER.next_serial(), event.time_msec(), event.cancelled());
            let pointer = state.pointer();
            pointer.gesture_pinch_end(state, &gesture::GesturePinchEndEvent { serial, time, cancelled });
            pointer.frame(state);
        }
        InputEvent::GestureHoldBegin { event } => {
            let fingers = event.fingers();
            let (serial, time) = (SERIAL_COUNTER.next_serial(), event.time_msec());
            let pointer = state.pointer();
            pointer.gesture_hold_begin(state, &gesture::GestureHoldBeginEvent { serial, time, fingers });
            pointer.frame(state);
        }
        InputEvent::GestureHoldEnd { event } => {
            let (serial, time, cancelled) = (SERIAL_COUNTER.next_serial(), event.time_msec(), event.cancelled());
            let pointer = state.pointer();
            pointer.gesture_hold_end(state, &gesture::GestureHoldEndEvent { serial, time, cancelled });
            pointer.frame(state);
        }
        InputEvent::TouchDown { event } => touch_down::<B>(state, event),
        InputEvent::TouchMotion { event } => touch_motion::<B>(state, event),
        InputEvent::TouchUp { event } => touch_up::<B>(state, event),
        InputEvent::TouchCancel { .. } => {
            if let Some(touch) = state.seat.get_touch() {
                touch.cancel(state);
            }
        }
        InputEvent::TouchFrame { .. } => {
            if let Some(touch) = state.seat.get_touch() {
                touch.frame(state);
            }
        }
        _ => {}
    }
}

fn swipe_begin<B: InputBackend>(state: &mut Slate, event: B::GestureSwipeBeginEvent) {
    let fingers = event.fingers();
    if state.lock.is_none() && fingers == state.settings.gestures.fingers {
        state.swipe = Some((0.0, 0.0));
        return;
    }
    let serial = SERIAL_COUNTER.next_serial();
    let pointer = state.pointer();
    pointer.gesture_swipe_begin(
        state,
        &gesture::GestureSwipeBeginEvent {
            serial,
            time: event.time_msec(),
            fingers,
        },
    );
    pointer.frame(state);
}

fn swipe_update<B: InputBackend>(state: &mut Slate, event: B::GestureSwipeUpdateEvent) {
    if let Some((x, y)) = state.swipe.as_mut() {
        *x += event.delta_x();
        *y += event.delta_y();
        return;
    }
    let pointer = state.pointer();
    pointer.gesture_swipe_update(
        state,
        &gesture::GestureSwipeUpdateEvent {
            time: event.time_msec(),
            delta: (event.delta_x(), event.delta_y()).into(),
        },
    );
    pointer.frame(state);
}

fn swipe_end<B: InputBackend>(state: &mut Slate, event: B::GestureSwipeEndEvent) {
    if let Some((x, y)) = state.swipe.take() {
        if event.cancelled() {
            return;
        }
        let gestures = &state.settings.gestures;
        if x.abs().max(y.abs()) < gestures.threshold {
            return;
        }
        let action = match (x.abs() >= y.abs(), x < 0.0, y < 0.0) {
            (true, true, _) => gestures.left.clone(),
            (true, false, _) => gestures.right.clone(),
            (false, _, true) => gestures.up.clone(),
            (false, _, false) => gestures.down.clone(),
        };
        if let Some(action) = action {
            run(state, action);
        }
        return;
    }
    let serial = SERIAL_COUNTER.next_serial();
    let pointer = state.pointer();
    pointer.gesture_swipe_end(
        state,
        &gesture::GestureSwipeEndEvent {
            serial,
            time: event.time_msec(),
            cancelled: event.cancelled(),
        },
    );
    pointer.frame(state);
}

fn touch_location<E: AbsolutePositionEvent<B>, B: InputBackend>(state: &Slate, event: &E) -> Point<f64, Logical> {
    let bounds = outputs_bounds(state);
    event.position_transformed(bounds.size) + bounds.loc.to_f64()
}

fn touch_down<B: InputBackend>(state: &mut Slate, event: B::TouchDownEvent) {
    let Some(touch) = state.seat.get_touch() else {
        return;
    };
    let location = touch_location::<_, B>(state, &event);
    let serial = SERIAL_COUNTER.next_serial();

    if state.lock.is_none() {
        let window = state.space.element_under(location).map(|(window, _)| window.clone());
        if let Some(window) = window {
            shell::focus_window(state, Some(&window), serial);
        }
    }
    let under = surface_under(state, location);
    touch.down(
        state,
        under,
        &DownEvent {
            slot: event.slot(),
            location,
            serial,
            time: event.time_msec(),
        },
    );
}

fn touch_motion<B: InputBackend>(state: &mut Slate, event: B::TouchMotionEvent) {
    let Some(touch) = state.seat.get_touch() else {
        return;
    };
    let location = touch_location::<_, B>(state, &event);
    let under = surface_under(state, location);
    touch.motion(
        state,
        under,
        &TouchMotionEvent {
            slot: event.slot(),
            location,
            time: event.time_msec(),
        },
    );
}

fn touch_up<B: InputBackend>(state: &mut Slate, event: B::TouchUpEvent) {
    let Some(touch) = state.seat.get_touch() else {
        return;
    };
    touch.up(
        state,
        &UpEvent {
            slot: event.slot(),
            serial: SERIAL_COUNTER.next_serial(),
            time: event.time_msec(),
        },
    );
}

fn mod_pressed(mod_key: ModKey, modifiers: &ModifiersState) -> bool {
    match mod_key {
        ModKey::Alt => modifiers.alt,
        ModKey::Logo => modifiers.logo,
    }
}

fn lookup(state: &Slate, modifiers: &ModifiersState, keysym: u32) -> Option<Action> {
    let primary_is_alt = matches!(state.mod_key, ModKey::Alt);
    let held = Required {
        shift: modifiers.shift,
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
        logo: modifiers.logo,
    };

    state
        .settings
        .bindings
        .iter()
        .find(|binding| binding.keysym == keysym && binding.modifiers.required(primary_is_alt) == held)
        .map(|binding| binding.action.clone())
}

fn keyboard_key<B: InputBackend>(state: &mut Slate, event: B::KeyboardKeyEvent) {
    let serial = SERIAL_COUNTER.next_serial();
    let time = Event::time_msec(&event);
    let key_state = event.state();
    let keyboard = state.keyboard();

    let pressed = keyboard.input(state, event.key_code(), key_state, serial, time, |state, modifiers, handle| {
        if switcher::active(state) {
            if key_state == KeyState::Released {
                return if switcher::released(state, modifiers) {
                    FilterResult::Intercept(Pressed::Switcher(SwitcherKey::Commit))
                } else {
                    FilterResult::Forward
                };
            }
            return FilterResult::Intercept(Pressed::Switcher(switcher_key(handle.modified_sym(), modifiers)));
        }
        if key_state != KeyState::Pressed {
            return FilterResult::Forward;
        }

        let vt = handle.modified_sym().raw();
        if (Keysym::XF86_Switch_VT_1.raw()..=Keysym::XF86_Switch_VT_12.raw()).contains(&vt) {
            return FilterResult::Intercept(Pressed::SwitchVt((vt - Keysym::XF86_Switch_VT_1.raw() + 1) as i32));
        }
        if state.lock.is_some() {
            return FilterResult::Forward;
        }

        let keysym = handle
            .raw_latin_sym_or_raw_current_sym()
            .unwrap_or(handle.modified_sym())
            .raw();
        match lookup(state, modifiers, bindings::normalize(keysym)) {
            Some(action) => FilterResult::Intercept(Pressed::Bound(action)),
            None => FilterResult::Forward,
        }
    });

    match pressed {
        Some(Pressed::SwitchVt(vt)) => udev::change_vt(state, vt),
        Some(Pressed::Bound(action)) => run(state, action),
        Some(Pressed::Switcher(key)) => match key {
            SwitcherKey::Step(forward) => switcher::step(state, forward),
            SwitcherKey::Move(dx, dy) => switcher::navigate(state, dx, dy),
            SwitcherKey::Commit => switcher::commit(state),
            SwitcherKey::Cancel => switcher::cancel(state),
            SwitcherKey::Ignore => {}
        },
        None => {}
    }
}

pub(super) fn run(state: &mut Slate, action: Action) {
    match action {
        Action::Terminal => state.spawn(&spawn::terminal_command(state.settings.terminal.as_deref(), "foot")),
        Action::Exec(command) => state.spawn(&command),
        Action::Close => {
            if let Some(window) = shell::focused_window(state) {
                shell::close_window(state, &window);
            }
        }
        Action::Maximize => {
            if let Some(window) = shell::focused_window(state) {
                shell::toggle_maximized(state, &window);
            }
        }
        Action::Minimize => {
            if let Some(window) = shell::focused_window(state) {
                shell::minimize_window(state, &window);
            }
        }
        Action::RestoreMinimized => shell::restore_last_minimized(state),
        Action::Fullscreen => {
            if let Some(window) = shell::focused_window(state) {
                shell::toggle_fullscreen(state, &window);
            }
        }
        Action::Quit => state.loop_signal.stop(),
        Action::WorkspaceNext => shell::switch_relative(state, true),
        Action::WorkspacePrevious => shell::switch_relative(state, false),
        Action::FocusMonitor(direction) => super::monitors::focus_monitor(state, direction),
        Action::MoveToMonitor(direction) => {
            if let Some(window) = shell::focused_window(state) {
                super::monitors::move_to_monitor(state, &window, direction);
            }
        }
        Action::Switcher | Action::SwitcherPrevious => {
            let modifiers = state.keyboard().modifier_state();
            switcher::start_cycle(state, action == Action::Switcher, &modifiers);
        }
        Action::SwitcherCommit => switcher::commit(state),
        Action::SwitcherCancel => switcher::cancel(state),
        Action::Overview => switcher::toggle_overview(state),
        Action::Scratchpad(name) => shell::toggle_scratchpad(state, &name),
        Action::Reload => state.reload_settings(),
        Action::Restart => tracing::warn!("restart is only supported by the X11 backend"),
        Action::Workspace(index) => shell::switch_workspace(state, index),
        Action::MoveToWorkspace(index) => {
            if let Some(window) = shell::focused_window(state) {
                shell::move_to_workspace(state, &window, index);
            }
        }
        Action::ToggleTiling => tiling::toggle_tiling(state),
        Action::LayoutNext => tiling::layout_next(state),
        Action::ToggleFloating => {
            if let Some(window) = shell::focused_window(state) {
                tiling::toggle_floating(state, &window);
            }
        }
        Action::MasterGrow => tiling::master_ratio(state, true),
        Action::MasterShrink => tiling::master_ratio(state, false),
        Action::MasterMore => tiling::master_count(state, true),
        Action::MasterFewer => tiling::master_count(state, false),
        Action::Promote => {
            if let Some(window) = shell::focused_window(state) {
                tiling::promote(state, &window);
            }
        }
        Action::SwapNext | Action::SwapPrevious => {
            if let Some(window) = shell::focused_window(state) {
                tiling::swap_neighbor(state, &window, action == Action::SwapNext);
            }
        }
        Action::FocusNext => shell::cycle_focus(state, true),
        Action::FocusPrevious => shell::cycle_focus(state, false),
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
    let pointer = state.pointer();
    let location = clamp_to_outputs(state, pointer.current_location() + event.delta());
    let serial = SERIAL_COUNTER.next_serial();
    if switcher::active(state) {
        switcher::hover(state, location);
        pointer.motion(state, None, &MotionEvent { location, serial, time: event.time_msec() });
        pointer.frame(state);
        return;
    }
    if !pointer.is_grabbed() {
        decoration::update_hover(state, location);
    }
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
    let pointer = state.pointer();
    if switcher::active(state) {
        switcher::hover(state, location);
        pointer.motion(state, None, &MotionEvent { location, serial, time: event.time_msec() });
        pointer.frame(state);
        return;
    }
    if !pointer.is_grabbed() {
        decoration::update_hover(state, location);
    }
    let under = surface_under(state, location);

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
    let pointer = state.pointer();
    let keyboard = state.keyboard();
    let serial = SERIAL_COUNTER.next_serial();
    let button = event.button_code();
    let button_state = event.state();
    if switcher::active(state) {
        if button_state == ButtonState::Pressed {
            switcher::click(state, pointer.current_location());
        }
        return;
    }

    if button_state == ButtonState::Released {
        decoration::release(state);
    }

    if button_state == ButtonState::Pressed && !pointer.is_grabbed() && state.lock.is_none() {
        let location = pointer.current_location();
        let decorated = if pointer.current_focus().is_none() {
            decoration::hit(state, location)
        } else {
            None
        };
        let window = state.space.element_under(location).map(|(window, _)| window.clone());

        if let Some((window, hit)) = decorated {
            decoration::press(state, window, hit, button, serial, location);
        } else if let Some(window) = window {
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
    if switcher::active(state) {
        return;
    }
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

    let pointer = state.pointer();
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
    if state.lock.is_some() {
        return super::lock::surface_for(state, output).map(|surface| (surface, output_location.to_f64()));
    }
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

    if decoration::hit(state, location).is_some() {
        return None;
    }

    if let Some((window, window_location)) = state.space.element_under(location)
        && let Some((surface, surface_location)) =
            window.surface_under(location - window_location.to_f64(), WindowSurfaceType::ALL)
    {
        return Some((surface, (surface_location + window_location).to_f64()));
    }

    layer_surface(Layer::Bottom).or_else(|| layer_surface(Layer::Background))
}
