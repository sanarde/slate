// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::input::InputEvent;
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::ImportDma;
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::{UdevBackend, UdevEvent, primary_gpu};
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use smithay::output::{Mode as OutputMode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::drm::control::{Device as ControlDevice, ModeTypeFlags, connector, crtc};
use smithay::reexports::input::{Device, Libinput};
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::utils::DeviceFd;

use super::input;
use super::render::{self, CLEAR_COLOR};
use super::state::Slate;
use crate::config::Settings;

const COLOR_FORMATS: [Fourcc; 4] = [Fourcc::Xrgb8888, Fourcc::Argb8888, Fourcc::Xbgr8888, Fourcc::Abgr8888];
const IDLE_REDRAW: Duration = Duration::from_millis(16);

type Compositor = DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

struct Surface {
    output: Output,
    global: GlobalId,
    connector: connector::Handle,
    compositor: Compositor,
    queued: bool,
    redraw_scheduled: bool,
}

pub struct Udev {
    session: LibSeatSession,
    libinput: Libinput,
    node: DrmNode,
    device: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
    renderer: GlesRenderer,
    devices: Vec<Device>,
    surfaces: HashMap<crtc::Handle, Surface>,
    active: bool,
}

pub fn init(event_loop: &EventLoop<'static, Slate>, state: &mut Slate) -> Result<(), Box<dyn Error>> {
    let (mut session, session_notifier) = LibSeatSession::new()?;
    let seat = session.seat();

    let udev_backend = UdevBackend::new(&seat)?;
    let path = match primary_gpu(&seat)? {
        Some(path) => path,
        None => udev_backend
            .device_list()
            .next()
            .map(|(_, path)| path.to_path_buf())
            .ok_or("no GPU found")?,
    };

    let fd = session.open(&path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK)?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (device, drm_notifier) = DrmDevice::new(drm_fd.clone(), true)?;
    let gbm = GbmDevice::new(drm_fd.clone())?;
    let node = DrmNode::from_file(&drm_fd)?;

    let egl = unsafe { EGLDisplay::new(gbm.clone())? };
    let context = EGLContext::new(&egl)?;
    let renderer = unsafe { GlesRenderer::new(context)? };

    let render_node = node
        .node_with_type(NodeType::Render)
        .and_then(Result::ok)
        .unwrap_or(node);
    let feedback = DmabufFeedbackBuilder::new(render_node.dev_id(), renderer.dmabuf_formats()).build()?;
    let dmabuf_global = state
        .dmabuf_state
        .create_global_with_default_feedback::<Slate>(&state.display_handle, &feedback);
    state.dmabuf_global = Some(dmabuf_global);

    let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    libinput
        .udev_assign_seat(&seat)
        .map_err(|()| "failed to assign the seat to libinput")?;
    let libinput_backend = LibinputInputBackend::new(libinput.clone());

    state.udev = Some(Udev {
        session,
        libinput,
        node,
        device,
        gbm,
        renderer,
        devices: Vec::new(),
        surfaces: HashMap::new(),
        active: true,
    });

    let handle = event_loop.handle();

    handle.insert_source(libinput_backend, |event, _, state| {
        match &event {
            InputEvent::DeviceAdded { device } => {
                if let Some(udev) = state.udev.as_mut() {
                    udev.add_device(device.clone(), &state.settings);
                }
            }
            InputEvent::DeviceRemoved { device } => {
                if let Some(udev) = state.udev.as_mut() {
                    udev.devices.retain(|known| known != device);
                }
            }
            _ => {}
        }
        input::process(state, event);
    })?;

    handle.insert_source(session_notifier, |event, _, state| match event {
        SessionEvent::PauseSession => pause(state),
        SessionEvent::ActivateSession => activate(state),
    })?;

    handle.insert_source(drm_notifier, |event, _, state| match event {
        DrmEvent::VBlank(crtc) => frame_submitted(state, crtc),
        DrmEvent::Error(error) => tracing::error!(%error, "DRM error"),
    })?;

    handle.insert_source(udev_backend, |event, _, state| {
        if let UdevEvent::Changed { device_id } = event {
            let ours = state.udev.as_ref().is_some_and(|udev| udev.node.dev_id() == device_id);
            if ours {
                scan_connectors(state);
            }
        }
    })?;

    scan_connectors(state);
    if state.udev.as_ref().is_some_and(|udev| udev.surfaces.is_empty()) {
        return Err("no connected output found".into());
    }

    Ok(())
}

fn configure_device(device: &mut Device, settings: &Settings) {
    if device.config_tap_finger_count() > 0 {
        let _ = device.config_tap_set_enabled(settings.tap_to_click);
    }
    if device.config_scroll_has_natural_scroll() {
        let _ = device.config_scroll_set_natural_scroll_enabled(settings.natural_scroll);
    }
}

impl Udev {
    fn add_device(&mut self, mut device: Device, settings: &Settings) {
        configure_device(&mut device, settings);
        self.devices.push(device);
    }

    pub fn apply_input_settings(&mut self, settings: &Settings) {
        for device in &mut self.devices {
            configure_device(device, settings);
        }
    }

    pub fn import_dmabuf(&mut self, dmabuf: &Dmabuf) -> bool {
        self.renderer.import_dmabuf(dmabuf, None).is_ok()
    }
}

pub fn change_vt(state: &mut Slate, vt: i32) {
    let Some(udev) = state.udev.as_mut() else {
        return;
    };
    if let Err(error) = udev.session.change_vt(vt) {
        tracing::warn!(%error, vt, "failed to switch virtual terminal");
    }
}

fn pause(state: &mut Slate) {
    let Some(udev) = state.udev.as_mut() else {
        return;
    };
    udev.active = false;
    udev.libinput.suspend();
    udev.device.pause();
}

fn activate(state: &mut Slate) {
    let Some(udev) = state.udev.as_mut() else {
        return;
    };
    if let Err(error) = udev.device.activate(false) {
        tracing::error!(%error, "failed to activate the DRM device");
        return;
    }
    if udev.libinput.resume().is_err() {
        tracing::error!("failed to resume libinput");
    }
    udev.active = true;

    let mut crtcs = Vec::new();
    for (crtc, surface) in udev.surfaces.iter_mut() {
        if let Err(error) = surface.compositor.reset_state() {
            tracing::warn!(%error, "failed to reset output state");
        }
        surface.queued = false;
        crtcs.push(*crtc);
    }
    for crtc in crtcs {
        render(state, crtc);
    }
}

fn scan_connectors(state: &mut Slate) {
    let Some(mut udev) = state.udev.take() else {
        return;
    };
    let added = scan(state, &mut udev);
    state.udev = Some(udev);

    for crtc in added {
        render(state, crtc);
    }
}

fn scan(state: &mut Slate, udev: &mut Udev) -> Vec<crtc::Handle> {
    let resources = match udev.device.resource_handles() {
        Ok(resources) => resources,
        Err(error) => {
            tracing::error!(%error, "failed to list DRM resources");
            return Vec::new();
        }
    };

    let connected: Vec<connector::Info> = resources
        .connectors()
        .iter()
        .filter_map(|handle| udev.device.get_connector(*handle, true).ok())
        .filter(|info| info.state() == connector::State::Connected)
        .collect();

    let gone: Vec<crtc::Handle> = udev
        .surfaces
        .iter()
        .filter(|(_, surface)| !connected.iter().any(|info| info.handle() == surface.connector))
        .map(|(crtc, _)| *crtc)
        .collect();
    for crtc in gone {
        let surface = udev.surfaces.remove(&crtc).unwrap();
        tracing::info!(output = surface.output.name(), "output removed");
        state.space.unmap_output(&surface.output);
        state.display_handle.disable_global::<Slate>(surface.global);
    }

    let mut added = Vec::new();
    for info in connected {
        if udev.surfaces.values().any(|surface| surface.connector == info.handle()) {
            continue;
        }
        match add_surface(state, udev, &resources, &info) {
            Ok(crtc) => added.push(crtc),
            Err(error) => tracing::error!(%error, "failed to set up output"),
        }
    }
    added
}

fn pick_crtc(
    udev: &Udev,
    resources: &smithay::reexports::drm::control::ResourceHandles,
    info: &connector::Info,
) -> Option<crtc::Handle> {
    let free = |crtc: &crtc::Handle| !udev.surfaces.contains_key(crtc);

    let current = info
        .current_encoder()
        .and_then(|encoder| udev.device.get_encoder(encoder).ok())
        .and_then(|encoder| encoder.crtc())
        .filter(free);
    if current.is_some() {
        return current;
    }

    info.encoders()
        .iter()
        .filter_map(|encoder| udev.device.get_encoder(*encoder).ok())
        .flat_map(|encoder| resources.filter_crtcs(encoder.possible_crtcs()))
        .find(free)
}

fn add_surface(
    state: &mut Slate,
    udev: &mut Udev,
    resources: &smithay::reexports::drm::control::ResourceHandles,
    info: &connector::Info,
) -> Result<crtc::Handle, Box<dyn Error>> {
    let mode = info
        .modes()
        .iter()
        .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        .or_else(|| info.modes().first())
        .copied()
        .ok_or("connector has no modes")?;
    let crtc = pick_crtc(udev, resources, info).ok_or("no free CRTC for the connector")?;

    let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
    let (width_mm, height_mm) = info.size().unwrap_or((0, 0));
    let output = Output::new(
        name.clone(),
        PhysicalProperties {
            size: (width_mm as i32, height_mm as i32).into(),
            subpixel: Subpixel::Unknown,
            make: "Unknown".to_string(),
            model: "Unknown".to_string(),
        },
    );
    let output_mode = OutputMode::from(mode);
    let x = state
        .space
        .outputs()
        .filter_map(|output| state.space.output_geometry(output))
        .map(|geometry| geometry.loc.x + geometry.size.w)
        .max()
        .unwrap_or(0);

    let global = output.create_global::<Slate>(&state.display_handle);
    output.change_current_state(Some(output_mode), None, None, Some((x, 0).into()));
    output.set_preferred(output_mode);
    state.space.map_output(&output, (x, 0));

    let drm_surface = udev.device.create_surface(crtc, mode, &[info.handle()])?;
    let allocator = GbmAllocator::new(udev.gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
    let import_node = udev
        .node
        .node_with_type(NodeType::Render)
        .and_then(Result::ok)
        .unwrap_or(udev.node);
    let exporter = GbmFramebufferExporter::new(udev.gbm.clone(), Some(import_node));

    let compositor = DrmCompositor::new(
        &output,
        drm_surface,
        None,
        allocator,
        exporter,
        COLOR_FORMATS,
        udev.renderer.dmabuf_formats(),
        udev.device.cursor_size(),
        Some(udev.gbm.clone()),
    )
    .map_err(|error| format!("failed to create the DRM compositor: {error}"))?;

    tracing::info!(output = name, mode = ?output_mode, "output added");
    udev.surfaces.insert(
        crtc,
        Surface {
            output,
            global,
            connector: info.handle(),
            compositor,
            queued: false,
            redraw_scheduled: false,
        },
    );
    Ok(crtc)
}

fn frame_submitted(state: &mut Slate, crtc: crtc::Handle) {
    let Some(udev) = state.udev.as_mut() else {
        return;
    };
    let Some(surface) = udev.surfaces.get_mut(&crtc) else {
        return;
    };
    if let Err(error) = surface.compositor.frame_submitted() {
        tracing::warn!(%error, "failed to mark the frame as submitted");
    }
    surface.queued = false;
    render(state, crtc);
}

fn render(state: &mut Slate, crtc: crtc::Handle) {
    let Some(mut udev) = state.udev.take() else {
        return;
    };

    if udev.active
        && let Some(surface) = udev.surfaces.get_mut(&crtc)
        && !surface.queued
    {
        render_surface(state, &mut udev.renderer, surface);
        if !surface.queued && !surface.redraw_scheduled {
            surface.redraw_scheduled = true;
            schedule_redraw(state, crtc);
        }
    }

    state.udev = Some(udev);
}

fn render_surface(state: &mut Slate, renderer: &mut GlesRenderer, surface: &mut Surface) {
    let elements = render::output_elements(state, renderer, &surface.output);
    let result = surface
        .compositor
        .render_frame(renderer, &elements, CLEAR_COLOR, FrameFlags::DEFAULT);

    match result {
        Ok(frame) => {
            let empty = frame.is_empty;
            drop(frame);
            if !empty {
                match surface.compositor.queue_frame(()) {
                    Ok(()) => surface.queued = true,
                    Err(error) => tracing::warn!(%error, "failed to queue the frame"),
                }
            }
        }
        Err(error) => tracing::warn!(%error, "failed to render the frame"),
    }

    state.send_frames(&surface.output);
}

fn schedule_redraw(state: &Slate, crtc: crtc::Handle) {
    let inserted = state.loop_handle.insert_source(Timer::from_duration(IDLE_REDRAW), move |_, _, state| {
        if let Some(surface) = state.udev.as_mut().and_then(|udev| udev.surfaces.get_mut(&crtc)) {
            surface.redraw_scheduled = false;
        }
        render(state, crtc);
        TimeoutAction::Drop
    });
    if let Err(error) = inserted {
        tracing::error!(%error, "failed to schedule a redraw");
    }
}
