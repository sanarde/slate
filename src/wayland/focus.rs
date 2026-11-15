// SPDX-License-Identifier: GPL-3.0-only

use std::borrow::Cow;

use smithay::backend::input::KeyState;
use smithay::desktop::{PopupKind, Window};
use smithay::input::Seat;
use smithay::input::keyboard::{KeyboardTarget, KeysymHandle, ModifiersState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Serial};
use smithay::wayland::seat::WaylandFocus;
use smithay::xwayland::X11Surface;

use super::state::Slate;

#[derive(Debug, Clone, PartialEq)]
pub enum KeyFocus {
    Wayland(WlSurface),
    X11(X11Surface),
}

impl KeyFocus {
    pub fn from_window(window: &Window) -> Option<KeyFocus> {
        if let Some(surface) = window.x11_surface() {
            return Some(KeyFocus::X11(surface.clone()));
        }
        window
            .toplevel()
            .map(|toplevel| KeyFocus::Wayland(toplevel.wl_surface().clone()))
    }
}

impl From<PopupKind> for KeyFocus {
    fn from(popup: PopupKind) -> KeyFocus {
        KeyFocus::Wayland(popup.wl_surface().clone())
    }
}

impl From<KeyFocus> for WlSurface {
    fn from(focus: KeyFocus) -> WlSurface {
        match focus {
            KeyFocus::Wayland(surface) => surface,
            KeyFocus::X11(surface) => surface
                .wl_surface()
                .expect("popup grabs are only started on Wayland surfaces"),
        }
    }
}

impl IsAlive for KeyFocus {
    fn alive(&self) -> bool {
        match self {
            KeyFocus::Wayland(surface) => surface.alive(),
            KeyFocus::X11(surface) => surface.alive(),
        }
    }
}

impl WaylandFocus for KeyFocus {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        match self {
            KeyFocus::Wayland(surface) => Some(Cow::Borrowed(surface)),
            KeyFocus::X11(surface) => surface.wl_surface().map(Cow::Owned),
        }
    }
}

impl KeyboardTarget<Slate> for KeyFocus {
    fn enter(&self, seat: &Seat<Slate>, data: &mut Slate, keys: Vec<KeysymHandle<'_>>, serial: Serial) {
        match self {
            KeyFocus::Wayland(surface) => KeyboardTarget::enter(surface, seat, data, keys, serial),
            KeyFocus::X11(surface) => KeyboardTarget::enter(surface, seat, data, keys, serial),
        }
    }

    fn leave(&self, seat: &Seat<Slate>, data: &mut Slate, serial: Serial) {
        match self {
            KeyFocus::Wayland(surface) => KeyboardTarget::leave(surface, seat, data, serial),
            KeyFocus::X11(surface) => KeyboardTarget::leave(surface, seat, data, serial),
        }
    }

    fn key(
        &self,
        seat: &Seat<Slate>,
        data: &mut Slate,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: u32,
    ) {
        match self {
            KeyFocus::Wayland(surface) => KeyboardTarget::key(surface, seat, data, key, state, serial, time),
            KeyFocus::X11(surface) => KeyboardTarget::key(surface, seat, data, key, state, serial, time),
        }
    }

    fn modifiers(&self, seat: &Seat<Slate>, data: &mut Slate, modifiers: ModifiersState, serial: Serial) {
        match self {
            KeyFocus::Wayland(surface) => KeyboardTarget::modifiers(surface, seat, data, modifiers, serial),
            KeyFocus::X11(surface) => KeyboardTarget::modifiers(surface, seat, data, modifiers, serial),
        }
    }
}
