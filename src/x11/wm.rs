// SPDX-License-Identifier: GPL-3.0-only

use std::error::Error;

use x11rb::connection::Connection;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

use super::atoms::Atoms;
use super::geometry::{Rect, Strut};
use super::keys;
use crate::spawn;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const BORDER: u32 = 2;
const BORDER_FOCUSED: u32 = 0x6750a4;
const BORDER_UNFOCUSED: u32 = 0x49454f;
const MIN_SIZE: u32 = 32;
const LEFT_PTR_GLYPH: u16 = 68;
const LOCK_VARIANTS: [u16; 4] = [0, 2, 16, 18];
const BUTTON_LEFT: u8 = 1;
const BUTTON_RIGHT: u8 = 3;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Normal,
    Dock,
    Desktop,
    Popup,
}

#[derive(Clone, Copy)]
enum Action {
    Terminal,
    Close,
    Maximize,
    Fullscreen,
    Quit,
}

struct Client {
    window: Window,
    rect: Rect,
    restore: Option<Rect>,
    maximized: bool,
    fullscreen: bool,
    delete: bool,
    take_focus: bool,
}

#[derive(Clone, Copy)]
struct Edges {
    left: bool,
    right: bool,
    top: bool,
    bottom: bool,
}

struct Drag {
    window: Window,
    resize: Option<Edges>,
    pointer: (i32, i32),
    initial: Rect,
}

pub struct Wm {
    conn: RustConnection,
    root: Window,
    screen: Rect,
    atoms: Atoms,
    bindings: Vec<(Keycode, Action)>,
    clients: Vec<Client>,
    docks: Vec<Window>,
    monitors: Vec<Rect>,
    work_areas: Vec<Rect>,
    drag: Option<Drag>,
    focused: Option<Window>,
    running: bool,
}

impl Wm {
    pub fn new(conn: RustConnection, screen_num: usize) -> Result<Wm> {
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let screen_rect = Rect::new(0, 0, screen.width_in_pixels as u32, screen.height_in_pixels as u32);

        let redirect = EventMask::SUBSTRUCTURE_REDIRECT
            | EventMask::SUBSTRUCTURE_NOTIFY
            | EventMask::PROPERTY_CHANGE
            | EventMask::BUTTON_PRESS;
        conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(redirect))?
            .check()
            .map_err(|_| "another window manager is running")?;

        let atoms = Atoms::new(&conn)?.reply()?;
        let mut wm = Wm {
            conn,
            root,
            screen: screen_rect,
            atoms,
            bindings: Vec::new(),
            clients: Vec::new(),
            docks: Vec::new(),
            monitors: Vec::new(),
            work_areas: Vec::new(),
            drag: None,
            focused: None,
            running: true,
        };

        wm.set_cursor()?;
        wm.publish_root_properties()?;
        wm.grab_keys()?;
        wm.conn
            .randr_select_input(root, randr::NotifyMask::SCREEN_CHANGE)?;
        wm.refresh_monitors()?;
        wm.adopt_existing()?;
        wm.conn.flush()?;
        Ok(wm)
    }

    pub fn run(&mut self) -> Result<()> {
        while self.running {
            let event = self.conn.wait_for_event()?;
            let mut next = Some(event);
            while let Some(event) = next {
                if let Err(error) = self.dispatch(event) {
                    tracing::debug!(%error, "event handling failed");
                }
                next = self.conn.poll_for_event()?;
            }
            self.conn.flush()?;
        }
        Ok(())
    }

    fn set_cursor(&self) -> Result<()> {
        let font = self.conn.generate_id()?;
        self.conn.open_font(font, b"cursor")?;
        let cursor = self.conn.generate_id()?;
        self.conn.create_glyph_cursor(
            cursor,
            font,
            font,
            LEFT_PTR_GLYPH,
            LEFT_PTR_GLYPH + 1,
            0,
            0,
            0,
            0xffff,
            0xffff,
            0xffff,
        )?;
        self.conn
            .change_window_attributes(self.root, &ChangeWindowAttributesAux::new().cursor(cursor))?;
        self.conn.close_font(font)?;
        Ok(())
    }

    fn publish_root_properties(&self) -> Result<()> {
        let atoms = &self.atoms;
        let check = self.conn.generate_id()?;
        self.conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            check,
            self.root,
            -1,
            -1,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )?;

        for window in [self.root, check] {
            self.conn
                .change_property32(PropMode::REPLACE, window, atoms._NET_SUPPORTING_WM_CHECK, AtomEnum::WINDOW, &[check])?;
        }
        self.conn
            .change_property8(PropMode::REPLACE, check, atoms._NET_WM_NAME, atoms.UTF8_STRING, b"slate")?;

        let supported = [
            atoms._NET_SUPPORTED,
            atoms._NET_SUPPORTING_WM_CHECK,
            atoms._NET_CLIENT_LIST,
            atoms._NET_ACTIVE_WINDOW,
            atoms._NET_NUMBER_OF_DESKTOPS,
            atoms._NET_CURRENT_DESKTOP,
            atoms._NET_DESKTOP_GEOMETRY,
            atoms._NET_WORKAREA,
            atoms._NET_CLOSE_WINDOW,
            atoms._NET_WM_STATE,
            atoms._NET_WM_STATE_FULLSCREEN,
            atoms._NET_WM_STATE_MAXIMIZED_VERT,
            atoms._NET_WM_STATE_MAXIMIZED_HORZ,
            atoms._NET_WM_WINDOW_TYPE,
            atoms._NET_WM_STRUT,
            atoms._NET_WM_STRUT_PARTIAL,
        ];
        self.conn
            .change_property32(PropMode::REPLACE, self.root, atoms._NET_SUPPORTED, AtomEnum::ATOM, &supported)?;
        self.conn
            .change_property32(PropMode::REPLACE, self.root, atoms._NET_NUMBER_OF_DESKTOPS, AtomEnum::CARDINAL, &[1])?;
        self.conn
            .change_property32(PropMode::REPLACE, self.root, atoms._NET_CURRENT_DESKTOP, AtomEnum::CARDINAL, &[0])?;
        self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            atoms._NET_DESKTOP_GEOMETRY,
            AtomEnum::CARDINAL,
            &[self.screen.w, self.screen.h],
        )?;
        Ok(())
    }

    fn grab_keys(&mut self) -> Result<()> {
        let wanted = [
            (keys::KEYSYM_RETURN, Action::Terminal),
            (keys::KEYSYM_Q, Action::Close),
            (keys::KEYSYM_M, Action::Maximize),
            (keys::KEYSYM_F, Action::Fullscreen),
            (keys::KEYSYM_E, Action::Quit),
        ];

        for (keysym, action) in wanted {
            let Some(keycode) = keys::keycode_for(&self.conn, keysym)? else {
                tracing::warn!(keysym, "key is not present in the keyboard mapping");
                continue;
            };
            let base = if matches!(action, Action::Quit) {
                u16::from(ModMask::M4) | u16::from(ModMask::SHIFT)
            } else {
                u16::from(ModMask::M4)
            };
            for lock in LOCK_VARIANTS {
                self.conn.grab_key(
                    true,
                    self.root,
                    ModMask::from(base | lock),
                    keycode,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )?;
            }
            self.bindings.push((keycode, action));
        }
        Ok(())
    }

    fn refresh_monitors(&mut self) -> Result<()> {
        let reply = self.conn.randr_get_monitors(self.root, true)?.reply()?;
        self.monitors = reply
            .monitors
            .iter()
            .map(|monitor| Rect::new(monitor.x as i32, monitor.y as i32, monitor.width as u32, monitor.height as u32))
            .collect();
        if self.monitors.is_empty() {
            self.monitors.push(self.screen);
        }
        self.update_work_areas()
    }

    fn bounds(&self) -> Rect {
        self.monitors
            .iter()
            .copied()
            .reduce(|bounds, monitor| bounds.union(&monitor))
            .unwrap_or(self.screen)
    }

    fn read_strut(&self, window: Window) -> Strut {
        let atoms = &self.atoms;
        for (atom, length) in [(atoms._NET_WM_STRUT_PARTIAL, 12), (atoms._NET_WM_STRUT, 4)] {
            let values: Vec<u32> = self
                .conn
                .get_property(false, window, atom, AtomEnum::CARDINAL, 0, length)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .and_then(|reply| reply.value32().map(|values| values.collect()))
                .unwrap_or_default();
            if values.len() >= 4 {
                return Strut {
                    left: values[0],
                    right: values[1],
                    top: values[2],
                    bottom: values[3],
                };
            }
        }
        Strut::default()
    }

    fn update_work_areas(&mut self) -> Result<()> {
        let mut strut = Strut::default();
        for dock in &self.docks {
            let dock_strut = self.read_strut(*dock);
            strut.left = strut.left.max(dock_strut.left);
            strut.right = strut.right.max(dock_strut.right);
            strut.top = strut.top.max(dock_strut.top);
            strut.bottom = strut.bottom.max(dock_strut.bottom);
        }

        let bounds = self.bounds();
        self.work_areas = self
            .monitors
            .iter()
            .map(|monitor| monitor.work_area(&bounds, &strut))
            .collect();

        let area = self.work_areas.first().copied().unwrap_or(self.screen);
        self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            self.atoms._NET_WORKAREA,
            AtomEnum::CARDINAL,
            &[area.x as u32, area.y as u32, area.w, area.h],
        )?;

        let refit: Vec<Window> = self
            .clients
            .iter()
            .filter(|client| client.maximized && !client.fullscreen)
            .map(|client| client.window)
            .collect();
        for window in refit {
            self.apply_state(window)?;
        }
        Ok(())
    }

    fn monitor_index(&self, rect: &Rect) -> usize {
        self.monitors
            .iter()
            .enumerate()
            .max_by_key(|(_, monitor)| monitor.overlap_area(rect))
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn pointer_monitor(&self) -> usize {
        let pointer = self.conn.query_pointer(self.root).ok().and_then(|cookie| cookie.reply().ok());
        let Some(pointer) = pointer else {
            return 0;
        };
        self.monitors
            .iter()
            .position(|monitor| monitor.contains(pointer.root_x as i32, pointer.root_y as i32))
            .unwrap_or(0)
    }

    fn adopt_existing(&mut self) -> Result<()> {
        let tree = self.conn.query_tree(self.root)?.reply()?;
        for window in tree.children {
            let attributes = self.conn.get_window_attributes(window)?.reply()?;
            if attributes.override_redirect || attributes.map_state != MapState::VIEWABLE {
                continue;
            }
            self.map_request(window)?;
        }
        Ok(())
    }

    fn dispatch(&mut self, event: Event) -> Result<()> {
        match event {
            Event::MapRequest(event) => self.map_request(event.window),
            Event::ConfigureRequest(event) => self.configure_request(&event),
            Event::UnmapNotify(event) => self.unmanage(event.window),
            Event::DestroyNotify(event) => self.unmanage(event.window),
            Event::ClientMessage(event) => self.client_message(&event),
            Event::PropertyNotify(event) => {
                if self.docks.contains(&event.window) {
                    self.update_work_areas()?;
                }
                Ok(())
            }
            Event::KeyPress(event) => self.key_press(&event),
            Event::ButtonPress(event) => self.button_press(&event),
            Event::MotionNotify(event) => self.motion(event.root_x as i32, event.root_y as i32),
            Event::ButtonRelease(_) => {
                self.drag = None;
                Ok(())
            }
            Event::RandrScreenChangeNotify(_) => self.refresh_monitors(),
            Event::Error(error) => {
                tracing::debug!(?error, "X11 error");
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn atoms_of(&self, window: Window, property: Atom) -> Vec<u32> {
        self.conn
            .get_property(false, window, property, AtomEnum::ATOM, 0, 32)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().map(|values| values.collect()))
            .unwrap_or_default()
    }

    fn kind_of(&self, window: Window) -> Kind {
        let atoms = &self.atoms;
        let types = self.atoms_of(window, atoms._NET_WM_WINDOW_TYPE);

        if types.contains(&atoms._NET_WM_WINDOW_TYPE_DOCK) {
            return Kind::Dock;
        }
        if types.contains(&atoms._NET_WM_WINDOW_TYPE_DESKTOP) {
            return Kind::Desktop;
        }
        let popups = [
            atoms._NET_WM_WINDOW_TYPE_NOTIFICATION,
            atoms._NET_WM_WINDOW_TYPE_TOOLTIP,
            atoms._NET_WM_WINDOW_TYPE_MENU,
            atoms._NET_WM_WINDOW_TYPE_DROPDOWN_MENU,
            atoms._NET_WM_WINDOW_TYPE_POPUP_MENU,
            atoms._NET_WM_WINDOW_TYPE_COMBO,
            atoms._NET_WM_WINDOW_TYPE_DND,
        ];
        if types.iter().any(|kind| popups.contains(kind)) {
            return Kind::Popup;
        }
        Kind::Normal
    }

    fn map_request(&mut self, window: Window) -> Result<()> {
        if self.clients.iter().any(|client| client.window == window) || self.docks.contains(&window) {
            self.conn.map_window(window)?;
            return Ok(());
        }

        match self.kind_of(window) {
            Kind::Normal => self.manage(window),
            Kind::Dock => {
                self.conn.change_window_attributes(
                    window,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                )?;
                self.conn.map_window(window)?;
                self.docks.push(window);
                self.update_work_areas()
            }
            Kind::Desktop => {
                let bounds = self.bounds();
                self.conn.configure_window(
                    window,
                    &ConfigureWindowAux::new()
                        .x(bounds.x)
                        .y(bounds.y)
                        .width(bounds.w)
                        .height(bounds.h)
                        .stack_mode(StackMode::BELOW),
                )?;
                self.conn.map_window(window)?;
                Ok(())
            }
            Kind::Popup => {
                self.conn
                    .configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
                self.conn.map_window(window)?;
                Ok(())
            }
        }
    }

    fn manage(&mut self, window: Window) -> Result<()> {
        let geometry = self.conn.get_geometry(window)?.reply()?;
        let protocols = self.atoms_of(window, self.atoms.WM_PROTOCOLS);

        let monitor = self.pointer_monitor();
        let area = self.work_areas.get(monitor).copied().unwrap_or(self.screen);
        let max_w = area.w.saturating_sub(2 * BORDER).max(MIN_SIZE);
        let max_h = area.h.saturating_sub(2 * BORDER).max(MIN_SIZE);
        let w = (geometry.width as u32).clamp(MIN_SIZE, max_w);
        let h = (geometry.height as u32).clamp(MIN_SIZE, max_h);

        let (x, y) = if geometry.x == 0 && geometry.y == 0 {
            (
                area.x + ((area.w.saturating_sub(w)) / 2) as i32,
                area.y + ((area.h.saturating_sub(h)) / 2) as i32,
            )
        } else {
            (geometry.x as i32, geometry.y as i32)
        };
        let rect = Rect::new(x, y, w, h);

        self.conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(rect.x)
                .y(rect.y)
                .width(rect.w)
                .height(rect.h)
                .border_width(BORDER),
        )?;
        self.conn.grab_button(
            false,
            window,
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::SYNC,
            GrabMode::ASYNC,
            NONE,
            NONE,
            ButtonIndex::ANY,
            ModMask::ANY,
        )?;

        self.clients.push(Client {
            window,
            rect,
            restore: None,
            maximized: false,
            fullscreen: false,
            delete: protocols.contains(&self.atoms.WM_DELETE_WINDOW),
            take_focus: protocols.contains(&self.atoms.WM_TAKE_FOCUS),
        });
        self.conn.map_window(window)?;
        self.update_client_list()?;
        self.focus(window)
    }

    fn unmanage(&mut self, window: Window) -> Result<()> {
        if let Some(index) = self.docks.iter().position(|dock| *dock == window) {
            self.docks.remove(index);
            return self.update_work_areas();
        }

        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        self.clients.remove(index);
        if self.drag.as_ref().is_some_and(|drag| drag.window == window) {
            self.drag = None;
        }
        self.update_client_list()?;

        if self.focused == Some(window) {
            self.focused = None;
            match self.clients.last().map(|client| client.window) {
                Some(top) => self.focus(top)?,
                None => {
                    self.conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, CURRENT_TIME)?;
                    self.set_active_window(NONE)?;
                }
            }
        }
        Ok(())
    }

    fn update_client_list(&self) -> Result<()> {
        let windows: Vec<Window> = self.clients.iter().map(|client| client.window).collect();
        self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            self.atoms._NET_CLIENT_LIST,
            AtomEnum::WINDOW,
            &windows,
        )?;
        Ok(())
    }

    fn set_active_window(&self, window: Window) -> Result<()> {
        self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            self.atoms._NET_ACTIVE_WINDOW,
            AtomEnum::WINDOW,
            &[window],
        )?;
        Ok(())
    }

    fn focus(&mut self, window: Window) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let client = self.clients.remove(index);
        let take_focus = client.take_focus;
        self.clients.push(client);

        self.conn.set_input_focus(InputFocus::POINTER_ROOT, window, CURRENT_TIME)?;
        if take_focus {
            self.send_protocol(window, self.atoms.WM_TAKE_FOCUS)?;
        }
        self.conn
            .configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        self.focused = Some(window);
        self.set_active_window(window)?;
        self.update_borders()
    }

    fn update_borders(&self) -> Result<()> {
        for client in &self.clients {
            let color = if self.focused == Some(client.window) {
                BORDER_FOCUSED
            } else {
                BORDER_UNFOCUSED
            };
            self.conn
                .change_window_attributes(client.window, &ChangeWindowAttributesAux::new().border_pixel(color))?;
        }
        Ok(())
    }

    fn send_protocol(&self, window: Window, protocol: Atom) -> Result<()> {
        let event = ClientMessageEvent::new(32, window, self.atoms.WM_PROTOCOLS, [protocol, CURRENT_TIME, 0, 0, 0]);
        self.conn.send_event(false, window, EventMask::NO_EVENT, event)?;
        Ok(())
    }

    fn close(&self, window: Window) -> Result<()> {
        let delete = self
            .clients
            .iter()
            .find(|client| client.window == window)
            .is_some_and(|client| client.delete);
        if delete {
            self.send_protocol(window, self.atoms.WM_DELETE_WINDOW)
        } else {
            self.conn.kill_client(window)?;
            Ok(())
        }
    }

    fn set_state(&mut self, window: Window, maximized: Option<bool>, fullscreen: Option<bool>) -> Result<()> {
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        let was_normal = !client.maximized && !client.fullscreen;
        if let Some(maximized) = maximized {
            client.maximized = maximized;
        }
        if let Some(fullscreen) = fullscreen {
            client.fullscreen = fullscreen;
        }
        let is_normal = !client.maximized && !client.fullscreen;
        if was_normal && !is_normal {
            client.restore = Some(client.rect);
        }
        self.apply_state(window)
    }

    fn apply_state(&mut self, window: Window) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let (maximized, fullscreen, current, restore) = {
            let client = &self.clients[index];
            (client.maximized, client.fullscreen, client.rect, client.restore)
        };
        let monitor = self.monitor_index(&current);

        let (rect, border) = if fullscreen {
            (self.monitors.get(monitor).copied().unwrap_or(self.screen), 0)
        } else if maximized {
            (self.work_areas.get(monitor).copied().unwrap_or(self.screen), 0)
        } else {
            (restore.unwrap_or(current), BORDER)
        };

        self.clients[index].rect = rect;
        self.conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(rect.x)
                .y(rect.y)
                .width(rect.w)
                .height(rect.h)
                .border_width(border),
        )?;

        let mut states = Vec::new();
        if fullscreen {
            states.push(self.atoms._NET_WM_STATE_FULLSCREEN);
        }
        if maximized {
            states.push(self.atoms._NET_WM_STATE_MAXIMIZED_VERT);
            states.push(self.atoms._NET_WM_STATE_MAXIMIZED_HORZ);
        }
        self.conn
            .change_property32(PropMode::REPLACE, window, self.atoms._NET_WM_STATE, AtomEnum::ATOM, &states)?;

        if fullscreen {
            self.conn
                .configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        }
        Ok(())
    }

    fn configure_request(&mut self, event: &ConfigureRequestEvent) -> Result<()> {
        let managed = self.clients.iter().position(|client| client.window == event.window);

        let Some(index) = managed else {
            let mut aux = ConfigureWindowAux::new();
            if event.value_mask.contains(ConfigWindow::X) {
                aux = aux.x(event.x as i32);
            }
            if event.value_mask.contains(ConfigWindow::Y) {
                aux = aux.y(event.y as i32);
            }
            if event.value_mask.contains(ConfigWindow::WIDTH) {
                aux = aux.width(event.width as u32);
            }
            if event.value_mask.contains(ConfigWindow::HEIGHT) {
                aux = aux.height(event.height as u32);
            }
            self.conn.configure_window(event.window, &aux)?;
            return Ok(());
        };

        let client = &mut self.clients[index];
        if client.maximized || client.fullscreen {
            return Ok(());
        }
        if event.value_mask.contains(ConfigWindow::X) {
            client.rect.x = event.x as i32;
        }
        if event.value_mask.contains(ConfigWindow::Y) {
            client.rect.y = event.y as i32;
        }
        if event.value_mask.contains(ConfigWindow::WIDTH) {
            client.rect.w = (event.width as u32).max(1);
        }
        if event.value_mask.contains(ConfigWindow::HEIGHT) {
            client.rect.h = (event.height as u32).max(1);
        }
        let rect = client.rect;
        self.conn.configure_window(
            event.window,
            &ConfigureWindowAux::new()
                .x(rect.x)
                .y(rect.y)
                .width(rect.w)
                .height(rect.h),
        )?;
        Ok(())
    }

    fn client_message(&mut self, event: &ClientMessageEvent) -> Result<()> {
        let atoms = &self.atoms;
        let data = event.data.as_data32();

        if event.type_ == atoms._NET_ACTIVE_WINDOW {
            return self.focus(event.window);
        }
        if event.type_ == atoms._NET_CLOSE_WINDOW {
            return self.close(event.window);
        }
        if event.type_ != atoms._NET_WM_STATE {
            return Ok(());
        }

        let Some(client) = self.clients.iter().find(|client| client.window == event.window) else {
            return Ok(());
        };
        let apply = |current: bool| match data[0] {
            0 => false,
            1 => true,
            _ => !current,
        };

        let mut maximized = None;
        let mut fullscreen = None;
        for property in [data[1], data[2]] {
            if property == atoms._NET_WM_STATE_FULLSCREEN {
                fullscreen = Some(apply(client.fullscreen));
            } else if property == atoms._NET_WM_STATE_MAXIMIZED_VERT || property == atoms._NET_WM_STATE_MAXIMIZED_HORZ {
                maximized = Some(apply(client.maximized));
            }
        }
        self.set_state(event.window, maximized, fullscreen)
    }

    fn key_press(&mut self, event: &KeyPressEvent) -> Result<()> {
        let Some((_, action)) = self.bindings.iter().find(|(keycode, _)| *keycode == event.detail) else {
            return Ok(());
        };
        let focused = self.focused;

        match *action {
            Action::Terminal => spawn::spawn(&spawn::terminal_command("xterm"), &[]),
            Action::Close => {
                if let Some(window) = focused {
                    self.close(window)?;
                }
            }
            Action::Maximize => {
                if let Some(client) = focused.and_then(|window| self.clients.iter().find(|client| client.window == window)) {
                    let maximized = !client.maximized;
                    self.set_state(client.window, Some(maximized), None)?;
                }
            }
            Action::Fullscreen => {
                if let Some(client) = focused.and_then(|window| self.clients.iter().find(|client| client.window == window)) {
                    let fullscreen = !client.fullscreen;
                    self.set_state(client.window, None, Some(fullscreen))?;
                }
            }
            Action::Quit => self.running = false,
        }
        Ok(())
    }

    fn button_press(&mut self, event: &ButtonPressEvent) -> Result<()> {
        let window = event.event;
        let Some(client) = self.clients.iter().find(|client| client.window == window) else {
            self.conn.allow_events(Allow::REPLAY_POINTER, CURRENT_TIME)?;
            return Ok(());
        };

        let mod_down = u16::from(event.state) & u16::from(ModMask::M4) != 0;
        let draggable = !client.maximized && !client.fullscreen;
        let initial = client.rect;
        self.focus(window)?;

        if !mod_down || !draggable || (event.detail != BUTTON_LEFT && event.detail != BUTTON_RIGHT) {
            let mode = if mod_down { Allow::ASYNC_POINTER } else { Allow::REPLAY_POINTER };
            self.conn.allow_events(mode, CURRENT_TIME)?;
            return Ok(());
        }

        let pointer = (event.root_x as i32, event.root_y as i32);
        let resize = (event.detail == BUTTON_RIGHT).then(|| {
            let center_x = initial.x + initial.w as i32 / 2;
            let center_y = initial.y + initial.h as i32 / 2;
            Edges {
                left: pointer.0 < center_x,
                right: pointer.0 >= center_x,
                top: pointer.1 < center_y,
                bottom: pointer.1 >= center_y,
            }
        });
        self.drag = Some(Drag {
            window,
            resize,
            pointer,
            initial,
        });
        self.conn.allow_events(Allow::ASYNC_POINTER, CURRENT_TIME)?;
        Ok(())
    }

    fn motion(&mut self, x: i32, y: i32) -> Result<()> {
        let Some(drag) = self.drag.as_ref() else {
            return Ok(());
        };
        let dx = x - drag.pointer.0;
        let dy = y - drag.pointer.1;
        let window = drag.window;
        let mut rect = drag.initial;

        match drag.resize {
            None => {
                rect.x += dx;
                rect.y += dy;
            }
            Some(edges) => {
                let mut width = rect.w as i32;
                let mut height = rect.h as i32;
                if edges.left {
                    width -= dx;
                }
                if edges.right {
                    width += dx;
                }
                if edges.top {
                    height -= dy;
                }
                if edges.bottom {
                    height += dy;
                }
                let width = width.max(MIN_SIZE as i32);
                let height = height.max(MIN_SIZE as i32);
                if edges.left {
                    rect.x += rect.w as i32 - width;
                }
                if edges.top {
                    rect.y += rect.h as i32 - height;
                }
                rect.w = width as u32;
                rect.h = height as u32;
            }
        }

        if let Some(client) = self.clients.iter_mut().find(|client| client.window == window) {
            client.rect = rect;
        }
        self.conn.configure_window(
            window,
            &ConfigureWindowAux::new()
                .x(rect.x)
                .y(rect.y)
                .width(rect.w)
                .height(rect.h),
        )?;
        Ok(())
    }
}
