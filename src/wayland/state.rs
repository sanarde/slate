// SPDX-License-Identifier: GPL-3.0-only

use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Instant;

use smithay::backend::renderer::gles::GlesTexProgram;
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::{PopupManager, Space, Window, layer_map_for_output};
use smithay::input::keyboard::KeyboardHandle;
use smithay::input::pointer::{CursorImageStatus, PointerHandle};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_server::backend::ClientData;
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState};
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::session_lock::{LockSurface, SessionLockManagerState};
use smithay::wayland::idle_notify::IdleNotifierState;
use smithay::wayland::pointer_gestures::PointerGesturesState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::single_pixel_buffer::SinglePixelBufferState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::xwayland_shell::XWaylandShellState;
use smithay::xwayland::X11Wm;

use super::animation::Animations;
use super::cursor::Cursors;
use super::shadow::{Key as ShadowKey, ShadowSet};
use super::decoration::Hit;
use super::settings::{resolve_mod_key, xkb_config};
use super::foreign_toplevel::ForeignToplevelState;
use super::grabs::Resizing;
use super::output_management::OutputManagementState;
use super::outputs::OutputRecord;
use super::screencopy::ScreencopyState;
use super::udev::Udev;
use crate::config::{ButtonKind, Settings};
use crate::font::Fonts;
use crate::ipc::Ipc;
use crate::layout::WorkspaceLayout;
use crate::source::ConfigSource;
use crate::spawn;

#[derive(Clone, Copy)]
pub enum ModKey {
    Alt,
    Logo,
}

pub struct Slate {
    pub start_time: Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, Slate>,
    pub loop_signal: LoopSignal,
    pub space: Space<Window>,
    pub pending_windows: Vec<Window>,
    pub parked: Vec<(Window, Point<i32, Logical>)>,
    pub output_workspaces: HashMap<String, usize>,
    pub minimized: Vec<(Window, Point<i32, Logical>)>,
    pub tile_order: Vec<Window>,
    pub animations: Animations,
    pub next_window_id: u64,
    pub last_focus: Option<u64>,
    pub swipe: Option<(f64, f64)>,
    pub closing: super::closing::Closing,
    pub switcher: Option<super::switcher::Switcher>,
    pub ipc: Option<Ipc>,
    pub foreign_toplevel: ForeignToplevelState,
    pub shadows: RefCell<HashMap<ShadowKey, ShadowSet>>,
    pub blur_program: RefCell<Option<Option<GlesTexProgram>>>,
    pub blur_pool: RefCell<super::blur::BlurPool>,
    pub clip_program: RefCell<Option<Option<GlesTexProgram>>>,
    pub clip_styles: RefCell<HashMap<(u64, String), u64>>,
    pub workspace_layouts: HashMap<String, Vec<WorkspaceLayout>>,
    pub outputs: Vec<OutputRecord>,
    pub output_management: OutputManagementState,
    pub screencopy: ScreencopyState,
    pub fractional_scale_state: FractionalScaleManagerState,
    pub session_lock_state: SessionLockManagerState,
    pub lock: Option<Vec<(Output, LockSurface)>>,
    pub xdg_decoration_state: XdgDecorationState,
    pub fonts: Fonts,
    pub decoration_generation: u64,
    pub decoration_hover: Option<(Window, Hit)>,
    pub decoration_press: Option<(Window, ButtonKind)>,
    pub decoration_cursor: bool,
    pub last_title_click: Option<(Window, Instant)>,
    pub resizing: Option<Resizing>,
    pub popups: PopupManager,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub xwayland_shell_state: XWaylandShellState,
    pub output_manager_state: OutputManagerState,
    pub shm_state: ShmState,
    pub dmabuf_state: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub seat_state: SeatState<Slate>,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    pub activation_state: XdgActivationState,
    pub idle_notifier_state: IdleNotifierState<Slate>,
    pub viewporter_state: ViewporterState,
    pub cursor_shape_state: CursorShapeManagerState,
    pub relative_pointer_state: RelativePointerManagerState,
    pub single_pixel_buffer_state: SinglePixelBufferState,
    pub pointer_gestures_state: PointerGesturesState,
    pub seat: Seat<Slate>,
    pub cursor_status: CursorImageStatus,
    pub cursors: Cursors,
    pub mod_key: ModKey,
    pub default_mod_key: ModKey,
    pub settings: Settings,
    pub config: ConfigSource,
    pub xwm: Option<X11Wm>,
    pub xdisplay: Option<u32>,
    pub udev: Option<Udev>,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {}

impl Slate {
    pub fn new(
        event_loop: &EventLoop<'static, Slate>,
        display: Display<Slate>,
        settings: Settings,
        config: ConfigSource,
        default_mod_key: ModKey,
    ) -> Result<Slate, Box<dyn Error>> {
        let display_handle = display.handle();
        let loop_handle = event_loop.handle();

        let socket = ListeningSocketSource::new_auto()?;
        let socket_name = socket.socket_name().to_os_string();

        loop_handle.insert_source(socket, |stream, _, state| {
            if let Err(error) = state.display_handle.insert_client(stream, Arc::new(ClientState::default())) {
                tracing::error!(%error, "failed to add client");
            }
        })?;

        loop_handle.insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, state| {
                unsafe { display.get_mut().dispatch_clients(state)? };
                Ok(PostAction::Continue)
            },
        )?;

        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(&display_handle, "seat0");
        seat.add_keyboard(xkb_config(&settings.xkb), settings.repeat_delay, settings.repeat_rate)?;
        seat.add_pointer();
        seat.add_touch();

        Ok(Slate {
            start_time: Instant::now(),
            socket_name,
            loop_handle,
            loop_signal: event_loop.get_signal(),
            space: Space::default(),
            pending_windows: Vec::new(),
            parked: Vec::new(),
            output_workspaces: HashMap::new(),
            minimized: Vec::new(),
            tile_order: Vec::new(),
            animations: Animations::default(),
            next_window_id: 0,
            last_focus: None,
            swipe: None,
            closing: Default::default(),
            switcher: None,
            ipc: None,
            foreign_toplevel: ForeignToplevelState::new(&display_handle),
            shadows: RefCell::new(HashMap::new()),
            blur_program: RefCell::new(None),
            blur_pool: RefCell::new(Default::default()),
            clip_program: RefCell::new(None),
            clip_styles: RefCell::new(HashMap::new()),
            workspace_layouts: HashMap::new(),
            outputs: Vec::new(),
            output_management: OutputManagementState::new(&display_handle),
            screencopy: ScreencopyState::new(&display_handle),
            fractional_scale_state: FractionalScaleManagerState::new::<Slate>(&display_handle),
            session_lock_state: SessionLockManagerState::new::<Slate, _>(&display_handle, |_| true),
            lock: None,
            xdg_decoration_state: XdgDecorationState::new::<Slate>(&display_handle),
            fonts: Fonts::load(&settings.decoration.title_font, &settings.decoration.icon_font),
            decoration_generation: 0,
            decoration_hover: None,
            decoration_press: None,
            decoration_cursor: false,
            last_title_click: None,
            resizing: None,
            popups: PopupManager::default(),
            compositor_state: CompositorState::new::<Slate>(&display_handle),
            xdg_shell_state: XdgShellState::new::<Slate>(&display_handle),
            layer_shell_state: WlrLayerShellState::new::<Slate>(&display_handle),
            xwayland_shell_state: XWaylandShellState::new::<Slate>(&display_handle),
            output_manager_state: OutputManagerState::new_with_xdg_output::<Slate>(&display_handle),
            shm_state: ShmState::new::<Slate>(&display_handle, vec![]),
            dmabuf_state: DmabufState::new(),
            dmabuf_global: None,
            data_device_state: DataDeviceState::new::<Slate>(&display_handle),
            primary_selection_state: PrimarySelectionState::new::<Slate>(&display_handle),
            activation_state: XdgActivationState::new::<Slate>(&display_handle),
            idle_notifier_state: IdleNotifierState::new(&display_handle, event_loop.handle()),
            viewporter_state: ViewporterState::new::<Slate>(&display_handle),
            cursor_shape_state: CursorShapeManagerState::new::<Slate>(&display_handle),
            relative_pointer_state: RelativePointerManagerState::new::<Slate>(&display_handle),
            single_pixel_buffer_state: SinglePixelBufferState::new::<Slate>(&display_handle),
            pointer_gestures_state: PointerGesturesState::new::<Slate>(&display_handle),
            seat_state,
            seat,
            cursor_status: CursorImageStatus::default_named(),
            cursors: Cursors::new(settings.cursor_theme.as_deref(), settings.cursor_size),
            mod_key: resolve_mod_key(settings.mod_key, default_mod_key),
            default_mod_key,
            settings,
            config,
            xwm: None,
            xdisplay: None,
            udev: None,
            display_handle,
        })
    }

    pub fn pointer(&self) -> PointerHandle<Slate> {
        self.seat.get_pointer().expect("the seat is created with a pointer")
    }

    pub fn keyboard(&self) -> KeyboardHandle<Slate> {
        self.seat.get_keyboard().expect("the seat is created with a keyboard")
    }

    pub fn spawn(&self, command: &[String]) {
        spawn::spawn(command, &self.spawn_environment());
    }

    pub fn spawn_environment(&self) -> Vec<(&'static str, OsString)> {
        let mut envs = vec![
            ("WAYLAND_DISPLAY", self.socket_name.clone()),
            ("XDG_SESSION_TYPE", OsString::from("wayland")),
        ];
        if let Some(number) = self.xdisplay {
            envs.push(("DISPLAY", OsString::from(format!(":{number}"))));
        }
        if let Some(ipc) = &self.ipc {
            envs.push(("SLATE_SOCKET", ipc.path().as_os_str().to_owned()));
        }
        if let Some(theme) = &self.settings.cursor_theme {
            envs.push(("XCURSOR_THEME", OsString::from(theme)));
        }
        if self.settings.cursor_size > 0 {
            envs.push(("XCURSOR_SIZE", OsString::from(self.settings.cursor_size.to_string())));
        }
        envs
    }

    pub fn send_frames(&self, output: &Output) {
        let elapsed = self.start_time.elapsed();

        for window in self.space.elements() {
            window.send_frame(output, elapsed, None, |_, _| Some(output.clone()));
        }

        if let Some(surface) = super::lock::surface_for(self, output) {
            send_frames_surface_tree(&surface, output, elapsed, None, |_, _| Some(output.clone()));
        }

        let layers = layer_map_for_output(output);
        for layer in layers.layers() {
            layer.send_frame(output, elapsed, None, |_, _| Some(output.clone()));
        }
    }
}
