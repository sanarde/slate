// SPDX-License-Identifier: GPL-3.0-only

use smithay::desktop::Window;
use smithay::input::Seat;
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, Focus, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
    GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
    GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Serial, Size};
use smithay::xwayland::xwm;

use super::state::Slate;
use super::tiling;

const MIN_SIZE: i32 = 32;

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl From<xdg_toplevel::ResizeEdge> for Edges {
    fn from(edge: xdg_toplevel::ResizeEdge) -> Edges {
        use xdg_toplevel::ResizeEdge::*;
        Edges {
            left: matches!(edge, Left | TopLeft | BottomLeft),
            right: matches!(edge, Right | TopRight | BottomRight),
            top: matches!(edge, Top | TopLeft | TopRight),
            bottom: matches!(edge, Bottom | BottomLeft | BottomRight),
        }
    }
}

impl From<xwm::ResizeEdge> for Edges {
    fn from(edge: xwm::ResizeEdge) -> Edges {
        use xwm::ResizeEdge::*;
        Edges {
            left: matches!(edge, Left | TopLeft | BottomLeft),
            right: matches!(edge, Right | TopRight | BottomRight),
            top: matches!(edge, Top | TopLeft | TopRight),
            bottom: matches!(edge, Bottom | BottomLeft | BottomRight),
        }
    }
}

impl Edges {
    pub fn new(left: bool, right: bool, top: bool, bottom: bool) -> Edges {
        Edges {
            left,
            right,
            top,
            bottom,
        }
    }

    pub fn any(&self) -> bool {
        self.left || self.right || self.top || self.bottom
    }

    pub fn nearest(geometry: Rectangle<i32, Logical>, point: Point<f64, Logical>) -> Edges {
        let x = point.x - geometry.loc.x as f64;
        let y = point.y - geometry.loc.y as f64;
        Edges {
            left: x < geometry.size.w as f64 / 2.0,
            right: x >= geometry.size.w as f64 / 2.0,
            top: y < geometry.size.h as f64 / 2.0,
            bottom: y >= geometry.size.h as f64 / 2.0,
        }
    }
}

pub struct Resizing {
    window: Window,
    edges: Edges,
    initial: Rectangle<i32, Logical>,
    released: bool,
}

enum GrabKind {
    Move,
    Resize(Edges),
}

struct WindowGrab {
    start_data: GrabStartData<Slate>,
    window: Window,
    initial: Rectangle<i32, Logical>,
    kind: GrabKind,
}

pub fn start_move(
    state: &mut Slate,
    seat: &Seat<Slate>,
    window: Window,
    serial: Serial,
    start_data: Option<GrabStartData<Slate>>,
) {
    start(state, seat, window, serial, start_data, GrabKind::Move);
}

pub fn start_resize(
    state: &mut Slate,
    seat: &Seat<Slate>,
    window: Window,
    edges: Edges,
    serial: Serial,
    start_data: Option<GrabStartData<Slate>>,
) {
    start(state, seat, window, serial, start_data, GrabKind::Resize(edges));
}

fn start(
    state: &mut Slate,
    seat: &Seat<Slate>,
    window: Window,
    serial: Serial,
    start_data: Option<GrabStartData<Slate>>,
    kind: GrabKind,
) {
    let pointer = seat.get_pointer().unwrap();
    let Some(mut start_data) = start_data.or_else(|| pointer.grab_start_data()) else {
        return;
    };
    if tiling::is_tiled(&window) {
        match kind {
            GrabKind::Move => tiling::make_floating(state, &window),
            GrabKind::Resize(_) => return,
        }
    }
    let Some(initial) = state.space.element_geometry(&window) else {
        return;
    };
    start_data.focus = None;

    if let GrabKind::Resize(edges) = kind {
        state.resizing = Some(Resizing {
            window: window.clone(),
            edges,
            initial,
            released: false,
        });
    }

    let grab = WindowGrab {
        start_data,
        window,
        initial,
        kind,
    };
    pointer.set_grab(state, grab, serial, Focus::Clear);
}

pub fn forget_window(state: &mut Slate, window: &Window) {
    if state.resizing.as_ref().is_some_and(|resizing| &resizing.window == window) {
        state.resizing = None;
    }
}

pub fn handle_resize_commit(state: &mut Slate, window: &Window) {
    let Some(resizing) = state.resizing.as_ref() else {
        return;
    };
    if &resizing.window != window || window.toplevel().is_none() {
        return;
    }

    let size = window.geometry().size;
    let mut location = resizing.initial.loc;
    if resizing.edges.left {
        location.x += resizing.initial.size.w - size.w;
    }
    if resizing.edges.top {
        location.y += resizing.initial.size.h - size.h;
    }
    let released = resizing.released;

    state.space.map_element(window.clone(), location, false);
    if released {
        state.resizing = None;
    }
}

impl WindowGrab {
    fn apply(&self, state: &mut Slate, location: Point<f64, Logical>) {
        let delta = (location - self.start_data.location).to_i32_round::<i32>();

        match self.kind {
            GrabKind::Move => {
                let position = self.initial.loc + delta;
                state.space.map_element(self.window.clone(), position, true);
                if let Some(surface) = self.window.x11_surface() {
                    let _ = surface.configure(Rectangle::new(position, self.initial.size));
                }
            }
            GrabKind::Resize(edges) => {
                let mut size = self.initial.size;
                if edges.left {
                    size.w -= delta.x;
                }
                if edges.right {
                    size.w += delta.x;
                }
                if edges.top {
                    size.h -= delta.y;
                }
                if edges.bottom {
                    size.h += delta.y;
                }
                let size = Size::from((size.w.max(MIN_SIZE), size.h.max(MIN_SIZE)));

                if let Some(toplevel) = self.window.toplevel() {
                    toplevel.with_pending_state(|pending| {
                        pending.states.set(xdg_toplevel::State::Resizing);
                        pending.size = Some(size);
                    });
                    toplevel.send_pending_configure();
                } else if let Some(surface) = self.window.x11_surface() {
                    let mut position = self.initial.loc;
                    if edges.left {
                        position.x += self.initial.size.w - size.w;
                    }
                    if edges.top {
                        position.y += self.initial.size.h - size.h;
                    }
                    let _ = surface.configure(Rectangle::new(position, size));
                    state.space.map_element(self.window.clone(), position, true);
                }
            }
        }
    }
}

impl PointerGrab<Slate> for WindowGrab {
    fn motion(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
        self.apply(data, event.location);
    }

    fn relative_motion(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(&mut self, data: &mut Slate, handle: &mut PointerInnerHandle<'_, Slate>, event: &ButtonEvent) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(&mut self, data: &mut Slate, handle: &mut PointerInnerHandle<'_, Slate>, details: AxisFrame) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut Slate, handle: &mut PointerInnerHandle<'_, Slate>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut Slate,
        handle: &mut PointerInnerHandle<'_, Slate>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &GrabStartData<Slate> {
        &self.start_data
    }

    fn unset(&mut self, data: &mut Slate) {
        if !matches!(self.kind, GrabKind::Resize(_)) {
            return;
        }
        if let Some(toplevel) = self.window.toplevel() {
            toplevel.with_pending_state(|pending| pending.states.unset(xdg_toplevel::State::Resizing));
            toplevel.send_pending_configure();
        }
        match data.resizing.as_mut() {
            Some(resizing) if self.window.toplevel().is_some() => resizing.released = true,
            _ => data.resizing = None,
        }
    }
}
