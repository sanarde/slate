// SPDX-License-Identifier: GPL-3.0-only

use std::sync::atomic::{AtomicBool, Ordering};

use rustix::time::{ClockId, clock_gettime};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::{Bind, ExportMem, Offscreen, TextureMapping};
use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_frame_v1::{
    self, Flags, ZwlrScreencopyFrameV1,
};
use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_manager_v1::{
    self, ZwlrScreencopyManagerV1,
};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};
use smithay::utils::{Buffer, Logical, Physical, Rectangle, Size, Transform};
use smithay::wayland::shm::{with_buffer_contents, with_buffer_contents_mut};

use super::render::{self, CLEAR_COLOR};
use super::state::Slate;

const VERSION: u32 = 3;
const BYTES_PER_PIXEL: i32 = 4;
const FORMAT: Fourcc = Fourcc::Argb8888;

pub struct FrameData {
    output: Option<Output>,
    rect: Rectangle<i32, Buffer>,
    overlay_cursor: bool,
    used: AtomicBool,
}

struct PendingCapture {
    frame: ZwlrScreencopyFrameV1,
    buffer: WlBuffer,
    output: Output,
    rect: Rectangle<i32, Buffer>,
    overlay_cursor: bool,
    with_damage: bool,
}

pub struct ScreencopyState {
    pending: Vec<PendingCapture>,
}

impl ScreencopyState {
    pub fn new(display: &DisplayHandle) -> ScreencopyState {
        display.create_global::<Slate, ZwlrScreencopyManagerV1, ()>(VERSION, ());
        ScreencopyState { pending: Vec::new() }
    }
}

pub fn cancel_output(state: &mut Slate, output: &Output) {
    state.screencopy.pending.retain(|capture| {
        let keep = &capture.output != output;
        if !keep && capture.frame.is_alive() {
            capture.frame.failed();
        }
        keep
    });
}

fn buffer_size(output: &Output) -> Option<Size<i32, Buffer>> {
    let mode = output.current_mode()?;
    let size = output.current_transform().transform_size(mode.size);
    Some((size.w, size.h).into())
}

fn region_in_buffer(output: &Output, region: Rectangle<i32, Logical>) -> Option<Rectangle<i32, Buffer>> {
    let mode = output.current_mode()?;
    let scale = output.current_scale().fractional_scale();
    let scaled: Rectangle<i32, Physical> = Rectangle::new(
        (
            (region.loc.x as f64 * scale).round() as i32,
            (region.loc.y as f64 * scale).round() as i32,
        )
            .into(),
        (
            (region.size.w as f64 * scale).round() as i32,
            (region.size.h as f64 * scale).round() as i32,
        )
            .into(),
    );
    let rotated = output.current_transform().transform_rect_in(scaled, &mode.size);
    let rect: Rectangle<i32, Buffer> = Rectangle::new(
        (rotated.loc.x, rotated.loc.y).into(),
        (rotated.size.w, rotated.size.h).into(),
    );

    let bounds = Rectangle::from_size(buffer_size(output)?);
    rect.intersection(bounds)
}

fn create_frame(
    data_init: &mut DataInit<'_, Slate>,
    frame: New<ZwlrScreencopyFrameV1>,
    overlay_cursor: bool,
    output: &WlOutput,
    region: Option<Rectangle<i32, Logical>>,
) {
    let output = Output::from_resource(output);
    let rect = output.as_ref().and_then(|output| match region {
        Some(region) => region_in_buffer(output, region),
        None => buffer_size(output).map(Rectangle::from_size),
    });
    let rect = rect.filter(|rect| rect.size.w > 0 && rect.size.h > 0);

    let frame = data_init.init(
        frame,
        FrameData {
            output: output.filter(|_| rect.is_some()),
            rect: rect.unwrap_or_default(),
            overlay_cursor,
            used: AtomicBool::new(false),
        },
    );

    match rect {
        Some(rect) => {
            frame.buffer(
                wl_shm::Format::Argb8888,
                rect.size.w as u32,
                rect.size.h as u32,
                (rect.size.w * BYTES_PER_PIXEL) as u32,
            );
            if frame.version() >= 3 {
                frame.buffer_done();
            }
        }
        None => frame.failed(),
    }
}

impl GlobalDispatch<ZwlrScreencopyManagerV1, ()> for Slate {
    fn bind(
        _state: &mut Slate,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrScreencopyManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Slate>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for Slate {
    fn request(
        _state: &mut Slate,
        _client: &Client,
        _manager: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, Slate>,
    ) {
        match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput {
                frame,
                overlay_cursor,
                output,
            } => create_frame(data_init, frame, overlay_cursor != 0, &output, None),
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                overlay_cursor,
                output,
                x,
                y,
                width,
                height,
            } => {
                let region = Rectangle::new((x, y).into(), (width, height).into());
                create_frame(data_init, frame, overlay_cursor != 0, &output, Some(region));
            }
            _ => {}
        }
    }
}

fn queue_copy(state: &mut Slate, frame: &ZwlrScreencopyFrameV1, data: &FrameData, buffer: WlBuffer, with_damage: bool) {
    if data.used.swap(true, Ordering::Relaxed) {
        frame.post_error(zwlr_screencopy_frame_v1::Error::AlreadyUsed, "the frame was already copied");
        return;
    }
    let Some(output) = data.output.clone() else {
        return;
    };

    let width = data.rect.size.w;
    let height = data.rect.size.h;
    let valid = with_buffer_contents(&buffer, |_, _, shm| {
        shm.width == width
            && shm.height == height
            && shm.stride >= width * BYTES_PER_PIXEL
            && matches!(shm.format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888)
    })
    .unwrap_or(false);
    if !valid {
        frame.post_error(
            zwlr_screencopy_frame_v1::Error::InvalidBuffer,
            "the buffer does not match the advertised size and format",
        );
        return;
    }

    state.screencopy.pending.push(PendingCapture {
        frame: frame.clone(),
        buffer,
        output,
        rect: data.rect,
        overlay_cursor: data.overlay_cursor,
        with_damage,
    });
}

impl Dispatch<ZwlrScreencopyFrameV1, FrameData> for Slate {
    fn request(
        state: &mut Slate,
        _client: &Client,
        frame: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        data: &FrameData,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, Slate>,
    ) {
        match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => queue_copy(state, frame, data, buffer, false),
            zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => queue_copy(state, frame, data, buffer, true),
            _ => {}
        }
    }
}

fn capture(state: &mut Slate, renderer: &mut GlesRenderer, capture: &PendingCapture) -> Result<bool, String> {
    let size = buffer_size(&capture.output).ok_or("the output has no mode")?;
    let mut texture: GlesTexture = renderer
        .create_buffer(FORMAT, size)
        .map_err(|error| format!("failed to create the capture buffer: {error}"))?;
    let mut framebuffer = renderer
        .bind(&mut texture)
        .map_err(|error| format!("failed to bind the capture buffer: {error}"))?;

    let elements = render::output_elements(state, renderer, &capture.output, capture.overlay_cursor);
    let mode = capture.output.current_mode().ok_or("the output has no mode")?;
    let transform = if state.udev.is_some() {
        capture.output.current_transform()
    } else {
        Transform::Normal
    };
    let scale = capture.output.current_scale().fractional_scale();
    OutputDamageTracker::new(mode.size, scale, transform)
        .render_output(renderer, &mut framebuffer, 0, &elements, CLEAR_COLOR)
        .map_err(|error| format!("failed to render the capture: {error}"))?;

    let mapping = renderer
        .copy_framebuffer(&framebuffer, capture.rect, FORMAT)
        .map_err(|error| format!("failed to read the capture: {error}"))?;
    let flipped = mapping.flipped();
    let pixels = renderer
        .map_texture(&mapping)
        .map_err(|error| format!("failed to map the capture: {error}"))?;

    let width = capture.rect.size.w;
    let height = capture.rect.size.h;
    let row_bytes = (width * BYTES_PER_PIXEL) as usize;
    if pixels.len() < row_bytes * height as usize {
        return Err("the capture is smaller than expected".to_string());
    }

    with_buffer_contents_mut(&capture.buffer, |pointer, length, shm| {
        for row in 0..height as usize {
            let offset = shm.offset as usize + row * shm.stride as usize;
            if offset + row_bytes > length {
                return Err("the buffer is too small".to_string());
            }
            let source = &pixels[row * row_bytes..(row + 1) * row_bytes];
            unsafe { std::ptr::copy_nonoverlapping(source.as_ptr(), pointer.add(offset), row_bytes) };
        }
        Ok(())
    })
    .map_err(|error| format!("failed to write the buffer: {error}"))??;

    Ok(flipped)
}

pub fn process(state: &mut Slate, renderer: &mut GlesRenderer, output: &Output) {
    if state.screencopy.pending.is_empty() {
        return;
    }

    let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut state.screencopy.pending)
        .into_iter()
        .partition(|pending| &pending.output == output);
    state.screencopy.pending = rest;

    for pending in mine {
        if !pending.frame.is_alive() {
            continue;
        }

        match capture(state, renderer, &pending) {
            Ok(flipped) => {
                if flipped {
                    pending.frame.flags(Flags::YInvert);
                }
                if pending.with_damage {
                    pending
                        .frame
                        .damage(0, 0, pending.rect.size.w as u32, pending.rect.size.h as u32);
                }
                let now = clock_gettime(ClockId::Monotonic);
                let seconds = now.tv_sec as u64;
                pending
                    .frame
                    .ready((seconds >> 32) as u32, seconds as u32, now.tv_nsec as u32);
            }
            Err(error) => {
                tracing::warn!(%error, "screen capture failed");
                pending.frame.failed();
            }
        }
    }
}
