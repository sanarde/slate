// SPDX-License-Identifier: GPL-3.0-only

use std::error::Error;

use std::collections::HashMap;
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use rustix::time::Timespec;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::properties::WmHints;
use x11rb::protocol::randr::{self, ConnectionExt as _};
use x11rb::protocol::shape;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

use super::atoms::Atoms;
use super::geometry::{Rect, Strut};
use super::keys;
mod frame;
mod ipc;
mod monitors;
mod tiling;

use frame::Insets;
use crate::bindings::{Action, Binding};
use crate::font::Fonts;
use crate::ipc::Ipc;
use crate::layout::WorkspaceLayout;
use crate::config::{ButtonKind, MAX_WORKSPACES, ModKeySetting, Settings};
use crate::source::ConfigSource;
use crate::rules::{self, Applied, Position};
use crate::spawn;

const POLL_SECONDS: i64 = 1;
const ICONIC_STATE: u32 = 3;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn mod_mask_of(setting: ModKeySetting) -> u16 {
    match setting {
        ModKeySetting::Alt => u16::from(ModMask::M1),
        ModKeySetting::Auto | ModKeySetting::Super => u16::from(ModMask::M4),
    }
}

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

struct Client {
    window: Window,
    rect: Rect,
    restore: Option<Rect>,
    maximized: bool,
    fullscreen: bool,
    delete: bool,
    take_focus: bool,
    workspace: usize,
    hidden: bool,
    minimized: bool,
    floating: bool,
    tiled: bool,
    float_rect: Option<Rect>,
    pending_unmaps: u32,
    accepts_input: bool,
    above: bool,
    frame: Option<Window>,
    title: String,
    painted: Option<frame::Painted>,
    shaped: Option<(i32, i32, i32)>,
    rule: Applied,
}

#[derive(Clone, Copy, PartialEq)]
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
    bindings: Vec<(Keycode, Binding)>,
    clients: Vec<Client>,
    docks: Vec<Window>,
    monitors: Vec<Rect>,
    work_areas: Vec<Rect>,
    drag: Option<Drag>,
    focused: Option<Window>,
    minimize_order: Vec<Window>,
    tile_order: Vec<Window>,
    workspace_layouts: Vec<Vec<WorkspaceLayout>>,
    monitor_workspaces: Vec<usize>,
    active_monitor: usize,
    settings: Settings,
    config: ConfigSource,
    mod_mask: u16,
    fonts: Fonts,
    depth: u8,
    gc: Gcontext,
    cursors: HashMap<u16, Cursor>,
    shape_available: bool,
    hover: Option<(Window, frame::FrameHit)>,
    pressed: Option<(Window, ButtonKind)>,
    last_title_click: Option<(Window, Instant)>,
    paint_generation: u64,
    ipc: Option<Ipc>,
    last_focus: Option<u64>,
    adopting: bool,
    running: bool,
}

impl Wm {
    pub fn new(conn: RustConnection, screen_num: usize, settings: Settings, config: ConfigSource) -> Result<Wm> {
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let screen_rect = Rect::new(0, 0, screen.width_in_pixels as u32, screen.height_in_pixels as u32);

        let redirect = EventMask::SUBSTRUCTURE_REDIRECT
            | EventMask::SUBSTRUCTURE_NOTIFY
            | EventMask::PROPERTY_CHANGE
            | EventMask::BUTTON_PRESS;
        let mut attempts = 0;
        while conn
            .change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(redirect))?
            .check()
            .is_err()
        {
            attempts += 1;
            if attempts > 20 {
                return Err("another window manager is running".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let atoms = Atoms::new(&conn)?.reply()?;
        let depth = conn.setup().roots[screen_num].root_depth;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, root, &CreateGCAux::new())?;
        let shape_available = conn
            .extension_information(shape::X11_EXTENSION_NAME)?
            .is_some();
        let fonts = Fonts::load(&settings.decoration.title_font, &settings.decoration.icon_font);
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
            minimize_order: Vec::new(),
            tile_order: Vec::new(),
            workspace_layouts: vec![vec![WorkspaceLayout::new(&settings.tiling); MAX_WORKSPACES]],
            monitor_workspaces: vec![0],
            active_monitor: 0,
            mod_mask: mod_mask_of(settings.mod_key),
            settings,
            config,
            fonts,
            depth,
            gc,
            cursors: HashMap::new(),
            shape_available,
            hover: None,
            pressed: None,
            last_title_click: None,
            paint_generation: 0,
            ipc: Ipc::start(),
            last_focus: None,
            adopting: false,
            running: true,
        };

        wm.monitor_workspaces[0] = wm.inherited_workspace();
        wm.set_cursor()?;
        wm.create_cursors()?;
        wm.publish_root_properties()?;
        wm.grab_keys()?;
        wm.conn
            .randr_select_input(root, randr::NotifyMask::SCREEN_CHANGE)?;
        wm.refresh_monitors()?;
        wm.adopt_existing()?;
        wm.conn.flush()?;
        Ok(wm)
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn environment(&self) -> Vec<(&'static str, std::ffi::OsString)> {
        let mut envs = Vec::new();
        if let Some(ipc) = &self.ipc {
            envs.push(("SLATE_SOCKET", ipc.path().as_os_str().to_owned()));
        }
        envs.push(("XDG_SESSION_TYPE", "x11".into()));
        envs
    }

    pub fn run(&mut self) -> Result<()> {
        let notify = self.config.notify_fd();
        let wake = self.ipc.as_ref().and_then(Ipc::wake_fd);
        let timeout = Timespec {
            tv_sec: POLL_SECONDS,
            tv_nsec: 0,
        };

        while self.running {
            while let Some(event) = self.conn.poll_for_event()? {
                if let Err(error) = self.dispatch(event) {
                    tracing::debug!(%error, "event handling failed");
                }
            }
            self.conn.flush()?;
            if !self.running {
                break;
            }

            let mut fds = vec![PollFd::new(self.conn.stream(), PollFlags::IN)];
            if let Some(notify) = &notify {
                fds.push(PollFd::new(notify, PollFlags::IN));
            }
            if let Some(wake) = &wake {
                fds.push(PollFd::new(wake, PollFlags::IN));
            }
            match poll(&mut fds, Some(&timeout)) {
                Ok(_) | Err(Errno::INTR) => {}
                Err(error) => return Err(error.into()),
            }
            self.serve_ipc();
            spawn::reap();
            self.reload_settings()?;
            self.refresh_monitors()?;
        }
        Ok(())
    }

    fn reload_settings(&mut self) -> Result<()> {
        if !self.config.changed() {
            return Ok(());
        }
        let settings = self.config.settings();
        self.apply_settings(settings)
    }

    fn apply_settings(&mut self, new: Settings) -> Result<()> {
        let old = std::mem::replace(&mut self.settings, new);
        tracing::info!("settings reloaded");
        let mod_mask = mod_mask_of(self.settings.mod_key);
        let workspaces_changed = old.workspace_count != self.settings.workspace_count;

        if workspaces_changed {
            self.clamp_workspaces()?;
            self.conn.change_property32(
                PropMode::REPLACE,
                self.root,
                self.atoms._NET_NUMBER_OF_DESKTOPS,
                AtomEnum::CARDINAL,
                &[self.settings.workspace_count as u32],
            )?;
        }
        if mod_mask != self.mod_mask || old.bindings != self.settings.bindings {
            self.mod_mask = mod_mask;
            self.conn.ungrab_key(0u8, self.root, ModMask::ANY)?;
            self.bindings.clear();
            self.grab_keys()?;
        }
        if old.tiling != self.settings.tiling {
            self.reset_layouts();
        }
        if old.border_width != self.settings.border_width || old.decoration != self.settings.decoration {
            if old.decoration.title_font != self.settings.decoration.title_font {
                self.fonts = Fonts::load(&self.settings.decoration.title_font, &self.settings.decoration.icon_font);
            }
            self.paint_generation += 1;
            let windows: Vec<Window> = self
                .clients
                .iter()
                .filter(|client| !client.hidden)
                .map(|client| client.window)
                .collect();
            for window in windows {
                self.sync_frame(window)?;
            }
            let windows: Vec<Window> = self.clients.iter().map(|client| client.window).collect();
            for window in windows {
                self.apply_state(window)?;
            }
        }
        self.arrange()?;
        self.update_borders()
    }

    fn clamp_workspaces(&mut self) -> Result<()> {
        let last = self.settings.workspace_count.saturating_sub(1);
        for monitor in 0..self.monitor_workspaces.len() {
            if self.monitor_workspaces[monitor] > last {
                self.switch_workspace_on(monitor, last)?;
            }
        }

        for index in 0..self.clients.len() {
            if self.clients[index].workspace <= last {
                continue;
            }
            let window = self.clients[index].window;
            self.clients[index].workspace = last;
            self.conn.change_property32(
                PropMode::REPLACE,
                window,
                self.atoms._NET_WM_DESKTOP,
                AtomEnum::CARDINAL,
                &[last as u32],
            )?;
            let monitor = self.monitor_index(&self.clients[index].rect);
            if last == self.workspace_on(monitor) && self.clients[index].hidden && !self.clients[index].minimized {
                self.sync_frame(window)?;
                self.clients[index].hidden = false;
                self.map_client(window)?;
            }
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

    fn create_cursors(&mut self) -> Result<()> {
        let font = self.conn.generate_id()?;
        self.conn.open_font(font, b"cursor")?;
        for glyph in [68u16, 134, 136, 12, 14, 70, 96, 138, 16] {
            let cursor = self.conn.generate_id()?;
            self.conn.create_glyph_cursor(
                cursor,
                font,
                font,
                glyph,
                glyph + 1,
                0,
                0,
                0,
                0xffff,
                0xffff,
                0xffff,
            )?;
            self.cursors.insert(glyph, cursor);
        }
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
            atoms._NET_WM_DESKTOP,
            atoms._NET_DESKTOP_GEOMETRY,
            atoms._NET_WORKAREA,
            atoms._NET_CLOSE_WINDOW,
            atoms._NET_WM_STATE,
            atoms._NET_WM_STATE_FULLSCREEN,
            atoms._NET_WM_STATE_ABOVE,
            atoms._NET_WM_STATE_HIDDEN,
            atoms._NET_WM_STATE_MAXIMIZED_VERT,
            atoms._NET_WM_STATE_MAXIMIZED_HORZ,
            atoms._NET_WM_WINDOW_TYPE,
            atoms._NET_WM_STRUT,
            atoms._NET_WM_STRUT_PARTIAL,
        ];
        self.conn
            .change_property32(PropMode::REPLACE, self.root, atoms._NET_SUPPORTED, AtomEnum::ATOM, &supported)?;
        self.conn
            .change_property32(PropMode::REPLACE, self.root, atoms._NET_NUMBER_OF_DESKTOPS, AtomEnum::CARDINAL, &[self.settings.workspace_count as u32])?;
        self.conn
            .change_property32(PropMode::REPLACE, self.root, atoms._NET_CURRENT_DESKTOP, AtomEnum::CARDINAL, &[self.workspace() as u32])?;
        self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            atoms._NET_DESKTOP_GEOMETRY,
            AtomEnum::CARDINAL,
            &[self.screen.w, self.screen.h],
        )?;
        Ok(())
    }

    fn primary_is_alt(&self) -> bool {
        self.settings.mod_key == ModKeySetting::Alt
    }

    fn mask_of(&self, binding: &Binding) -> u16 {
        let required = binding.modifiers.required(self.primary_is_alt());
        let mut mask = 0;
        if required.shift {
            mask |= u16::from(ModMask::SHIFT);
        }
        if required.ctrl {
            mask |= u16::from(ModMask::CONTROL);
        }
        if required.alt {
            mask |= u16::from(ModMask::M1);
        }
        if required.logo {
            mask |= u16::from(ModMask::M4);
        }
        mask
    }

    fn grab_keys(&mut self) -> Result<()> {
        for binding in self.settings.bindings.clone() {
            let Some(keycode) = keys::keycode_for(&self.conn, binding.keysym)? else {
                tracing::warn!(keysym = binding.keysym, "key is not present in the keyboard mapping");
                continue;
            };

            let mask = self.mask_of(&binding);
            for lock in LOCK_VARIANTS {
                self.conn.grab_key(
                    true,
                    self.root,
                    ModMask::from(mask | lock),
                    keycode,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )?;
            }
            self.bindings.push((keycode, binding));
        }
        Ok(())
    }

    fn refresh_monitors(&mut self) -> Result<()> {
        let previous = self.monitors.clone();
        let reply = self.conn.randr_get_monitors(self.root, true)?.reply()?;
        self.monitors = reply
            .monitors
            .iter()
            .map(|monitor| Rect::new(monitor.x as i32, monitor.y as i32, monitor.width as u32, monitor.height as u32))
            .collect();
        if self.monitors.is_empty() {
            self.monitors.push(self.screen);
        }
        if self.monitors == previous {
            return Ok(());
        }
        let count = self.monitors.len();
        self.monitor_workspaces.resize(count, 0);
        let tiling = self.settings.tiling.clone();
        self.workspace_layouts
            .resize_with(count, || vec![WorkspaceLayout::new(&tiling); MAX_WORKSPACES]);
        self.active_monitor = self.active_monitor.min(count - 1);
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
        self.arrange()
    }

    fn monitor_index(&self, rect: &Rect) -> usize {
        self.monitors
            .iter()
            .enumerate()
            .max_by_key(|(_, monitor)| monitor.overlap_area(rect))
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    pub(super) fn pointer_monitor(&self) -> usize {
        let pointer = self.conn.query_pointer(self.root).ok().and_then(|cookie| cookie.reply().ok());
        let Some(pointer) = pointer else {
            return 0;
        };
        self.monitors
            .iter()
            .position(|monitor| monitor.contains(pointer.root_x as i32, pointer.root_y as i32))
            .unwrap_or(0)
    }

    fn workspace(&self) -> usize {
        self.workspace_on(self.active_monitor)
    }

    fn workspace_on(&self, monitor: usize) -> usize {
        self.monitor_workspaces.get(monitor).copied().unwrap_or(0)
    }

    fn visible_on(&self, client: &Client, monitor: usize) -> bool {
        !client.minimized
            && self.monitor_index(&client.rect) == monitor
            && (client.rule.sticky || client.workspace == self.workspace_on(monitor))
    }

    fn adopt_monitor_workspace(&mut self, window: Window) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let monitor = self.monitor_index(&self.clients[index].rect);
        let workspace = self.workspace_on(monitor);
        if self.clients[index].rule.sticky || self.clients[index].workspace == workspace || self.clients[index].hidden {
            return Ok(());
        }
        self.clients[index].workspace = workspace;
        self.conn.change_property32(
            PropMode::REPLACE,
            window,
            self.atoms._NET_WM_DESKTOP,
            AtomEnum::CARDINAL,
            &[workspace as u32],
        )?;
        Ok(())
    }

    fn inherited_workspace(&self) -> usize {
        let desktop = self
            .conn
            .get_property(false, self.root, self.atoms._NET_CURRENT_DESKTOP, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().and_then(|mut values| values.next()));
        desktop
            .map(|desktop| desktop as usize)
            .filter(|desktop| *desktop < self.settings.workspace_count)
            .unwrap_or(0)
    }

    fn desktop_of(&self, window: Window) -> Option<u32> {
        self.conn
            .get_property(false, window, self.atoms._NET_WM_DESKTOP, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().and_then(|mut values| values.next()))
    }

    fn adopt_existing(&mut self) -> Result<()> {
        let tree = self.conn.query_tree(self.root)?.reply()?;
        let mut moves = Vec::new();
        self.adopting = true;
        for window in tree.children {
            let attributes = self.conn.get_window_attributes(window)?.reply()?;
            if attributes.override_redirect || attributes.map_state != MapState::VIEWABLE {
                continue;
            }
            let desktop = self.desktop_of(window);
            self.map_request(window)?;
            if let Some(desktop) = desktop
                && self.clients.iter().any(|client| client.window == window)
            {
                moves.push((window, desktop as usize));
            }
        }
        self.adopting = false;

        for (window, desktop) in moves {
            if desktop < self.settings.workspace_count && desktop != self.workspace() {
                self.move_to_workspace(window, desktop)?;
            }
        }
        Ok(())
    }

    fn dispatch(&mut self, event: Event) -> Result<()> {
        match event {
            Event::MapRequest(event) => self.map_request(event.window),
            Event::ConfigureRequest(event) => self.configure_request(&event),
            Event::UnmapNotify(event) => self.unmapped(event.window),
            Event::DestroyNotify(event) => self.unmanage(event.window),
            Event::ClientMessage(event) => self.client_message(&event),
            Event::PropertyNotify(event) => {
                if self.docks.contains(&event.window) {
                    self.update_work_areas()?;
                } else if event.atom == self.atoms._NET_WM_NAME || event.atom == u32::from(AtomEnum::WM_NAME) {
                    self.update_title(event.window)?;
                }
                Ok(())
            }
            Event::Expose(event) => {
                if event.count == 0
                    && let Some(window) = self.client_of_frame(event.window)
                {
                    if let Some(client) = self.clients.iter_mut().find(|client| client.window == window) {
                        client.painted = None;
                    }
                    self.repaint(window)?;
                }
                Ok(())
            }
            Event::LeaveNotify(event) => self.frame_leave(event.event),
            Event::KeyPress(event) => self.key_press(&event),
            Event::ButtonPress(event) => self.button_press(&event),
            Event::MotionNotify(event) => {
                if self.drag.is_some() {
                    self.motion(event.root_x as i32, event.root_y as i32)
                } else {
                    self.frame_motion(&event)
                }
            }
            Event::ButtonRelease(_) => {
                if let Some(drag) = self.drag.take() {
                    self.adopt_monitor_workspace(drag.window)?;
                }
                self.frame_release()
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
        if !self.adopting {
            self.active_monitor = monitor;
        }
        let area = self.work_areas.get(monitor).copied().unwrap_or(self.screen);
        let border = self.settings.border_width;
        let title = if self.should_frame(window) {
            self.settings.decoration.titlebar_height
        } else {
            0
        };
        let max_w = area.w.saturating_sub(2 * border).max(MIN_SIZE);
        let max_h = area.h.saturating_sub(2 * border + title).max(MIN_SIZE);
        let applied = rules::apply(&self.settings.window_rules, &self.class_of(window), &self.read_title(window));
        let fresh = !self.adopting;
        let (wanted_w, wanted_h) = match applied.size {
            Some((width, height)) if fresh => (width, height),
            _ => (geometry.width as u32, geometry.height as u32),
        };
        let w = wanted_w.clamp(MIN_SIZE, max_w);
        let h = wanted_h.clamp(MIN_SIZE, max_h);

        let (x, y) = if let (Some(Position::At(px, py)), true) = (applied.position, fresh) {
            (area.x + px + border as i32, area.y + py + (border + title) as i32)
        } else if (geometry.x == 0 && geometry.y == 0) || (fresh && applied.position == Some(Position::Center)) {
            (
                area.x + ((area.w.saturating_sub(w)) / 2) as i32,
                area.y + ((area.h.saturating_sub(h)) / 2) as i32,
            )
        } else {
            (geometry.x as i32, geometry.y as i32)
        };
        let rect = Rect::new(x, y, w, h);
        let mapped = self.conn.get_window_attributes(window)?.reply()?.map_state == MapState::VIEWABLE;

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

        let hints = WmHints::get(&self.conn, window)
            .ok()
            .and_then(|cookie| cookie.reply().ok().flatten());
        let accepts_input = hints.and_then(|hints| hints.input).unwrap_or(true);
        let initial_state = self.atoms_of(window, self.atoms._NET_WM_STATE);

        self.clients.push(Client {
            window,
            rect,
            restore: None,
            maximized: false,
            fullscreen: false,
            delete: protocols.contains(&self.atoms.WM_DELETE_WINDOW),
            take_focus: protocols.contains(&self.atoms.WM_TAKE_FOCUS),
            workspace: self.workspace(),
            hidden: false,
            minimized: false,
            floating: false,
            tiled: false,
            float_rect: None,
            pending_unmaps: 0,
            accepts_input,
            above: applied.above || initial_state.contains(&self.atoms._NET_WM_STATE_ABOVE),
            frame: None,
            title: String::new(),
            painted: None,
            shaped: None,
            rule: applied.clone(),
        });
        let desktop = if applied.sticky { u32::MAX } else { self.workspace() as u32 };
        self.conn.change_property32(
            PropMode::REPLACE,
            window,
            self.atoms._NET_WM_DESKTOP,
            AtomEnum::CARDINAL,
            &[desktop],
        )?;
        if let Some(opacity) = applied.opacity {
            self.conn.change_property32(
                PropMode::REPLACE,
                window,
                self.atoms._NET_WM_WINDOW_OPACITY,
                AtomEnum::CARDINAL,
                &[(f64::from(opacity) * f64::from(u32::MAX)) as u32],
            )?;
        }
        self.frame_client(window, mapped)?;
        if self.clients.iter().any(|client| client.window == window && client.frame.is_none()) {
            self.set_geometry(window, rect, border)?;
        }
        self.map_client(window)?;
        self.update_client_list()?;
        self.register_tiled(window);
        let forced = applied.floating.or((applied.sticky || applied.scratchpad.is_some()).then_some(true));
        if let (Some(floating), Some(client)) = (forced, self.clients.iter_mut().find(|client| client.window == window)) {
            client.floating = floating;
        }
        self.focus(window)?;

        let fullscreen = initial_state.contains(&self.atoms._NET_WM_STATE_FULLSCREEN) || (fresh && applied.fullscreen);
        let maximized = (initial_state.contains(&self.atoms._NET_WM_STATE_MAXIMIZED_VERT)
            && initial_state.contains(&self.atoms._NET_WM_STATE_MAXIMIZED_HORZ))
            || (fresh && applied.maximize);
        if fullscreen || maximized {
            self.set_state(window, Some(maximized), Some(fullscreen))?;
        }
        self.arrange()?;
        self.window_event("window-open", window);
        self.restack()?;

        if fresh
            && !applied.sticky
            && let Some(target) = applied.workspace.filter(|target| *target < self.settings.workspace_count)
            && target != self.workspace()
        {
            self.move_to_workspace(window, target)?;
        }
        Ok(())
    }

    fn toggle_scratchpad(&mut self, name: &str) -> Result<()> {
        let current = self.workspace();
        let found = self
            .clients
            .iter()
            .find(|client| client.rule.scratchpad.as_deref() == Some(name))
            .map(|client| (client.window, client.minimized, client.hidden));

        match found {
            Some((window, false, false)) => {
                if self.focused == Some(window) {
                    self.minimize(window)
                } else {
                    self.focus(window)
                }
            }
            Some((window, true, _)) => {
                self.bring_to_active_monitor(window)?;
                if let Some(client) = self.clients.iter_mut().find(|client| client.window == window) {
                    client.workspace = current;
                }
                self.restore(window)
            }
            Some((window, false, true)) => {
                self.bring_to_active_monitor(window)?;
                if let Some(client) = self.clients.iter_mut().find(|client| client.window == window) {
                    client.workspace = current;
                }
                self.conn.change_property32(
                    PropMode::REPLACE,
                    window,
                    self.atoms._NET_WM_DESKTOP,
                    AtomEnum::CARDINAL,
                    &[current as u32],
                )?;
                self.sync_frame(window)?;
                self.set_hidden(window, false);
                self.map_client(window)?;
                self.arrange()?;
                self.focus(window)
            }
            None => {
                match self.settings.scratchpads.iter().find(|(pad, _)| pad == name) {
                    Some((_, command)) => spawn::spawn(command, &[]),
                    None => tracing::warn!(name, "no scratchpad with this name"),
                }
                Ok(())
            }
        }
    }

    fn restart(&mut self) -> Result<()> {
        std::fs::File::open("/proc/self/exe").map_err(|error| format!("cannot restart: {error}"))?;
        let clients: Vec<(Window, Option<Window>, Rect)> = self
            .clients
            .iter()
            .map(|client| (client.window, client.frame, client.rect))
            .collect();
        for (window, frame, rect) in clients {
            if let Some(frame) = frame {
                self.conn.reparent_window(window, self.root, rect.x as i16, rect.y as i16)?;
                self.conn.destroy_window(frame)?;
            }
            self.conn.map_window(window)?;
        }
        self.conn.flush()?;

        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new("/proc/self/exe");
        command
            .arg0(std::env::args_os().next().unwrap_or_default())
            .args(std::env::args_os().skip(1))
            .env(crate::session::RESTART_VARIABLE, "1");
        let error = command.exec();
        Err(error.into())
    }

    fn unmapped(&mut self, window: Window) -> Result<()> {
        if let Some(client) = self.clients.iter_mut().find(|client| client.window == window)
            && client.pending_unmaps > 0
        {
            client.pending_unmaps -= 1;
            return Ok(());
        }
        self.unmanage(window)
    }

    fn switch_workspace(&mut self, target: usize) -> Result<()> {
        self.switch_workspace_on(self.active_monitor, target)
    }

    fn switch_workspace_on(&mut self, monitor: usize, target: usize) -> Result<()> {
        if target >= self.settings.workspace_count || target == self.workspace_on(monitor) {
            return Ok(());
        }
        if let Some(slot) = self.monitor_workspaces.get_mut(monitor) {
            *slot = target;
        }
        for index in 0..self.clients.len() {
            if self.clients[index].rule.sticky && self.monitor_index(&self.clients[index].rect) == monitor {
                self.clients[index].workspace = target;
            }
        }

        let changes: Vec<(Window, bool)> = self
            .clients
            .iter()
            .filter(|client| self.monitor_index(&client.rect) == monitor)
            .filter_map(|client| {
                let visible = (client.workspace == target || client.rule.sticky) && !client.minimized;
                if visible && client.hidden {
                    Some((client.window, true))
                } else if !visible && !client.hidden {
                    Some((client.window, false))
                } else {
                    None
                }
            })
            .collect();
        for (window, show) in changes {
            if show {
                self.sync_frame(window)?;
                self.set_hidden(window, false);
                self.map_client(window)?;
            } else {
                self.set_hidden(window, true);
                self.unmap_client(window)?;
            }
        }
        if monitor == self.active_monitor {
            self.conn.change_property32(
                PropMode::REPLACE,
                self.root,
                self.atoms._NET_CURRENT_DESKTOP,
                AtomEnum::CARDINAL,
                &[target as u32],
            )?;
        }
        self.arrange()?;
        self.emit(
            "workspace",
            &[
                ("index", (target + 1).to_string()),
                ("output", crate::json::quote(&format!("monitor-{}", monitor + 1))),
            ],
        );

        let focus_here = self
            .focused
            .and_then(|window| self.clients.iter().find(|client| client.window == window))
            .is_none_or(|client| self.monitor_index(&client.rect) == monitor);
        if focus_here {
            self.focused = None;
            return self.refocus(monitor);
        }
        Ok(())
    }

    fn refocus(&mut self, monitor: usize) -> Result<()> {
        match self.focus_candidate(monitor) {
            Some(window) => self.focus(window),
            None => {
                self.conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, CURRENT_TIME)?;
                self.set_active_window(NONE)
            }
        }
    }

    fn move_to_workspace(&mut self, window: Window, target: usize) -> Result<()> {
        if target >= self.settings.workspace_count {
            return Ok(());
        }
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        if client.rule.sticky {
            return Ok(());
        }
        client.workspace = target;
        let monitor = self.monitor_index(&self.clients.iter().find(|client| client.window == window).unwrap().rect);
        self.conn.change_property32(
            PropMode::REPLACE,
            window,
            self.atoms._NET_WM_DESKTOP,
            AtomEnum::CARDINAL,
            &[target as u32],
        )?;
        if target == self.workspace_on(monitor) {
            return Ok(());
        }

        if let Some(client) = self.clients.iter_mut().find(|client| client.window == window) {
            client.hidden = true;
        }
        self.unmap_client(window)?;

        self.arrange()?;
        self.focused = None;
        self.refocus(monitor)
    }

    fn unmanage(&mut self, window: Window) -> Result<()> {
        if let Some(index) = self.docks.iter().position(|dock| *dock == window) {
            self.docks.remove(index);
            return self.update_work_areas();
        }

        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        self.emit("window-close", &[("id", window.to_string())]);
        let removed = self.clients.remove(index);
        let removed_monitor = self.monitor_index(&removed.rect);
        if let Some(frame) = removed.frame {
            self.conn
                .reparent_window(window, self.root, removed.rect.x as i16, removed.rect.y as i16)?;
            self.conn.destroy_window(frame)?;
        }
        if self.hover.as_ref().is_some_and(|(hovered, _)| *hovered == window) {
            self.hover = None;
        }
        if self.pressed.as_ref().is_some_and(|(pressed, _)| *pressed == window) {
            self.pressed = None;
        }
        if self.last_title_click.as_ref().is_some_and(|(clicked, _)| *clicked == window) {
            self.last_title_click = None;
        }
        self.minimize_order.retain(|minimized| *minimized != window);
        self.forget_tiled(window);
        if self.drag.as_ref().is_some_and(|drag| drag.window == window) {
            self.drag = None;
        }
        self.update_client_list()?;

        if self.focused == Some(window) {
            self.focused = None;
            match self.focus_candidate(removed_monitor) {
                Some(top) => self.focus(top)?,
                None => {
                    self.conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, CURRENT_TIME)?;
                    self.set_active_window(NONE)?;
                }
            }
        }
        self.arrange()
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
        self.minimize_order.retain(|minimized| *minimized != window);
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let workspace = self.clients[index].workspace;
        let monitor = self.monitor_index(&self.clients[index].rect);
        if workspace != self.workspace_on(monitor) && !self.clients[index].minimized && !self.clients[index].rule.sticky {
            self.switch_workspace_on(monitor, workspace)?;
        }
        self.active_monitor = monitor;
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let client = self.clients.remove(index);
        let take_focus = client.take_focus;
        let accepts_input = client.accepts_input;
        self.clients.push(client);

        let top = self.top_of(window);
        self.conn
            .configure_window(top, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        if !accepts_input && !take_focus {
            return self.restack();
        }
        if accepts_input {
            self.conn.set_input_focus(InputFocus::POINTER_ROOT, window, CURRENT_TIME)?;
        }
        if take_focus {
            self.send_protocol(window, self.atoms.WM_TAKE_FOCUS)?;
        }
        self.focused = Some(window);
        if self.last_focus != Some(window as u64) {
            self.last_focus = Some(window as u64);
            self.emit("focus", &[("id", window.to_string())]);
        }
        self.set_active_window(window)?;
        self.update_borders()?;
        self.restack()
    }

    fn minimize(&mut self, window: Window) -> Result<()> {
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        if client.minimized {
            return Ok(());
        }
        client.minimized = true;
        let was_hidden = client.hidden;
        client.hidden = true;
        if !was_hidden {
            self.unmap_client(window)?;
        }
        self.minimize_order.push(window);
        self.apply_state(window)?;
        self.arrange()?;

        if self.focused == Some(window) {
            self.focused = None;
            let monitor = self.clients.iter().find(|client| client.window == window).map_or(0, |client| self.monitor_index(&client.rect));
            match self.focus_candidate(monitor) {
                Some(top) => self.focus(top)?,
                None => {
                    self.conn.set_input_focus(InputFocus::POINTER_ROOT, self.root, CURRENT_TIME)?;
                    self.set_active_window(NONE)?;
                    self.update_borders()?;
                }
            }
        }
        Ok(())
    }

    fn restore(&mut self, window: Window) -> Result<()> {
        let Some(client) = self.clients.iter_mut().find(|client| client.window == window) else {
            return Ok(());
        };
        if !client.minimized {
            return Ok(());
        }
        client.minimized = false;
        let workspace = client.workspace;
        let rect = client.rect;
        self.minimize_order.retain(|minimized| *minimized != window);

        let monitor = self.monitor_index(&rect);
        if workspace != self.workspace_on(monitor) {
            self.switch_workspace_on(monitor, workspace)?;
        } else if self.clients.iter().any(|client| client.window == window && client.hidden) {
            self.sync_frame(window)?;
            self.set_hidden(window, false);
            self.map_client(window)?;
        }
        self.apply_state(window)?;
        self.arrange()?;
        self.focus(window)
    }

    fn focus_candidate(&self, monitor: usize) -> Option<Window> {
        self.clients
            .iter()
            .rev()
            .find(|client| self.visible_on(client, monitor) && (client.accepts_input || client.take_focus))
            .map(|client| client.window)
    }

    fn restack(&self) -> Result<()> {
        let fullscreen = self
            .clients
            .iter()
            .any(|client| client.fullscreen && !client.hidden && Some(client.window) == self.focused);
        if !fullscreen {
            for dock in &self.docks {
                self.conn
                    .configure_window(*dock, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
            }
        }
        for client in self.clients.iter().filter(|client| client.above && !client.hidden) {
            self.conn.configure_window(
                client.frame.unwrap_or(client.window),
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        }
        Ok(())
    }

    fn set_hidden(&mut self, window: Window, hidden: bool) {
        if let Some(client) = self.clients.iter_mut().find(|client| client.window == window) {
            client.hidden = hidden;
        }
    }

    fn update_borders(&mut self) -> Result<()> {
        let windows: Vec<(Window, bool)> = self.clients.iter().map(|client| (client.window, client.frame.is_some())).collect();
        for (window, framed) in windows {
            if framed {
                self.repaint(window)?;
                continue;
            }
            let color = if self.focused == Some(window) {
                self.settings.border_focused
            } else {
                self.settings.border_unfocused
            };
            self.conn
                .change_window_attributes(window, &ChangeWindowAttributesAux::new().border_pixel(color))?;
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
        self.apply_state(window)?;
        self.arrange()
    }

    fn apply_state(&mut self, window: Window) -> Result<()> {
        let Some(index) = self.clients.iter().position(|client| client.window == window) else {
            return Ok(());
        };
        let (maximized, fullscreen, above, minimized, current, restore) = {
            let client = &self.clients[index];
            (
                client.maximized,
                client.fullscreen,
                client.above,
                client.minimized,
                client.rect,
                client.restore,
            )
        };
        let monitor = self.monitor_index(&current);

        let (rect, border) = if fullscreen {
            (self.monitors.get(monitor).copied().unwrap_or(self.screen), 0)
        } else if maximized {
            let area = self.work_areas.get(monitor).copied().unwrap_or(self.screen);
            let insets = if self.clients[index].frame.is_some() {
                self.insets_when_framed(true, false)
            } else {
                Insets::default()
            };
            (
                Rect::new(
                    area.x + insets.left,
                    area.y + insets.top,
                    (area.w as i32 - insets.left - insets.right).max(1) as u32,
                    (area.h as i32 - insets.top - insets.bottom).max(1) as u32,
                ),
                0,
            )
        } else {
            (restore.unwrap_or(current), self.settings.border_width)
        };

        self.set_geometry(window, rect, border)?;

        let mut states = Vec::new();
        if fullscreen {
            states.push(self.atoms._NET_WM_STATE_FULLSCREEN);
        }
        if above {
            states.push(self.atoms._NET_WM_STATE_ABOVE);
        }
        if minimized {
            states.push(self.atoms._NET_WM_STATE_HIDDEN);
        }
        if maximized {
            states.push(self.atoms._NET_WM_STATE_MAXIMIZED_VERT);
            states.push(self.atoms._NET_WM_STATE_MAXIMIZED_HORZ);
        }
        self.conn
            .change_property32(PropMode::REPLACE, window, self.atoms._NET_WM_STATE, AtomEnum::ATOM, &states)?;

        if fullscreen {
            let top = self.top_of(window);
            self.conn
                .configure_window(top, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
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
        if client.maximized || client.fullscreen || client.tiled {
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
        let border = self.settings.border_width;
        self.set_geometry(event.window, rect, border)
    }

    fn client_message(&mut self, event: &ClientMessageEvent) -> Result<()> {
        let atoms = &self.atoms;
        let data = event.data.as_data32();

        if event.type_ == atoms._NET_ACTIVE_WINDOW {
            if self.clients.iter().any(|client| client.window == event.window && client.minimized) {
                return self.restore(event.window);
            }
            return self.focus(event.window);
        }
        if event.type_ == atoms.WM_CHANGE_STATE {
            if data[0] == ICONIC_STATE {
                return self.minimize(event.window);
            }
            return Ok(());
        }
        if event.type_ == atoms._NET_CLOSE_WINDOW {
            return self.close(event.window);
        }
        if event.type_ == atoms._NET_CURRENT_DESKTOP {
            return self.switch_workspace(data[0] as usize);
        }
        if event.type_ == atoms._NET_WM_DESKTOP {
            return self.move_to_workspace(event.window, data[0] as usize);
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
        let mut above = None;
        for property in [data[1], data[2]] {
            if property == atoms._NET_WM_STATE_FULLSCREEN {
                fullscreen = Some(apply(client.fullscreen));
            } else if property == atoms._NET_WM_STATE_ABOVE {
                above = Some(apply(client.above));
            } else if property == atoms._NET_WM_STATE_MAXIMIZED_VERT || property == atoms._NET_WM_STATE_MAXIMIZED_HORZ {
                maximized = Some(apply(client.maximized));
            }
        }
        if let Some(above) = above
            && let Some(client) = self.clients.iter_mut().find(|client| client.window == event.window)
        {
            client.above = above;
        }
        self.set_state(event.window, maximized, fullscreen)?;
        self.restack()
    }

    fn cycle_focus(&mut self, forward: bool) -> Result<()> {
        let monitor = self.active_monitor;
        let candidates: Vec<Window> = self
            .clients
            .iter()
            .filter(|client| self.visible_on(client, monitor) && !client.hidden && (client.accepts_input || client.take_focus))
            .map(|client| client.window)
            .collect();
        if candidates.len() < 2 {
            return Ok(());
        }

        if forward {
            return self.focus(candidates[0]);
        }
        let top = candidates[candidates.len() - 1];
        let top_window = self.top_of(top);
        self.conn
            .configure_window(top_window, &ConfigureWindowAux::new().stack_mode(StackMode::BELOW))?;
        if let Some(index) = self.clients.iter().position(|client| client.window == top) {
            let client = self.clients.remove(index);
            self.clients.insert(0, client);
        }
        self.focus(candidates[candidates.len() - 2])
    }

    fn key_press(&mut self, event: &KeyPressEvent) -> Result<()> {
        if let Some(monitor) = self.monitors.iter().position(|monitor| monitor.contains(event.root_x as i32, event.root_y as i32)) {
            self.active_monitor = monitor;
        }
        let held = u16::from(event.state)
            & (u16::from(ModMask::SHIFT) | u16::from(ModMask::CONTROL) | u16::from(ModMask::M1) | u16::from(ModMask::M4));
        let action = self
            .bindings
            .iter()
            .find(|(keycode, binding)| *keycode == event.detail && self.mask_of(binding) == held)
            .map(|(_, binding)| binding.action.clone());
        match action {
            Some(action) => self.run_action(action),
            None => Ok(()),
        }
    }

    fn run_action(&mut self, action: Action) -> Result<()> {
        let focused = self.focused;

        match action {
            Action::Terminal => {
                let environment = self.spawn_env();
                spawn::spawn(&spawn::terminal_command(self.settings.terminal.as_deref(), "xterm"), &environment);
            }
            Action::Exec(command) => {
                let environment = self.spawn_env();
                spawn::spawn(&command, &environment);
            }
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
            Action::Minimize => {
                if let Some(window) = focused {
                    self.minimize(window)?;
                }
            }
            Action::RestoreMinimized => {
                if let Some(window) = self.minimize_order.last().copied() {
                    self.restore(window)?;
                }
            }
            Action::Quit => self.running = false,
            Action::WorkspaceNext => {
                let target = (self.workspace() + 1).min(self.settings.workspace_count.saturating_sub(1));
                self.switch_workspace(target)?
            }
            Action::WorkspacePrevious => self.switch_workspace(self.workspace().saturating_sub(1))?,
            Action::FocusMonitor(direction) => self.focus_monitor(direction)?,
            Action::MoveToMonitor(direction) => {
                if let Some(window) = self.focused {
                    self.move_to_monitor(window, direction)?;
                }
            }
            Action::Switcher => self.cycle_focus(true)?,
            Action::SwitcherPrevious => self.cycle_focus(false)?,
            Action::SwitcherCommit | Action::SwitcherCancel | Action::Overview => {
                tracing::debug!("the switcher and the overview are Wayland only");
            }
            Action::Reload => self.reload_settings()?,
            Action::Restart => self.restart()?,
            Action::Scratchpad(name) => self.toggle_scratchpad(&name)?,
            Action::Workspace(index) => self.switch_workspace(index)?,
            Action::MoveToWorkspace(index) => {
                if let Some(window) = focused {
                    self.move_to_workspace(window, index)?;
                }
            }
            Action::ToggleTiling => self.toggle_tiling()?,
            Action::LayoutNext => self.layout_next()?,
            Action::ToggleFloating => {
                if let Some(window) = focused {
                    self.toggle_floating(window)?;
                }
            }
            Action::MasterGrow => self.master_ratio(true)?,
            Action::MasterShrink => self.master_ratio(false)?,
            Action::MasterMore => self.master_count(true)?,
            Action::MasterFewer => self.master_count(false)?,
            Action::Promote => {
                if let Some(window) = focused {
                    self.promote(window)?;
                }
            }
            Action::SwapNext => {
                if let Some(window) = focused {
                    self.swap_neighbor(window, true)?;
                }
            }
            Action::SwapPrevious => {
                if let Some(window) = focused {
                    self.swap_neighbor(window, false)?;
                }
            }
            Action::FocusNext => self.cycle_focus(true)?,
            Action::FocusPrevious => self.cycle_focus(false)?,
        }
        Ok(())
    }

    fn button_press(&mut self, event: &ButtonPressEvent) -> Result<()> {
        if self.client_of_frame(event.event).is_some() {
            return self.frame_press(event);
        }
        let window = event.event;
        let Some(client) = self.clients.iter().find(|client| client.window == window) else {
            self.conn.allow_events(Allow::REPLAY_POINTER, CURRENT_TIME)?;
            return Ok(());
        };

        let mod_down = u16::from(event.state) & self.mod_mask != 0;
        let tiled = client.tiled;
        let draggable = !client.maximized && !client.fullscreen && !(tiled && event.detail == BUTTON_RIGHT);
        self.focus(window)?;
        if tiled && mod_down && event.detail == BUTTON_LEFT {
            self.make_floating(window)?;
        }
        let initial = self
            .clients
            .iter()
            .find(|client| client.window == window)
            .map(|client| client.rect)
            .unwrap_or_default();

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

        let border = self.settings.border_width;
        self.set_geometry(window, rect, border)?;
        Ok(())
    }
}
