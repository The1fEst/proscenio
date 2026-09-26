use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs::File;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::rc::Rc;
use std::time::{Duration, Instant};
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, delegate_noop, event_created_child, globals,
};

use crate::platform::gbm::{self, Gbm};
use crate::platform::readable;
use crate::platform::vulkan;

pub mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-toplevel-export-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-toplevel-export-v1.xml");
}

pub mod dmabuf {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/linux-dmabuf-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/linux-dmabuf-v1.xml");
}

use dmabuf::zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1};
use dmabuf::zwp_linux_dmabuf_feedback_v1::{self, ZwpLinuxDmabufFeedbackV1};
use dmabuf::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use protocol::hyprland_toplevel_export_frame_v1::{self, HyprlandToplevelExportFrameV1};
use protocol::hyprland_toplevel_export_manager_v1::HyprlandToplevelExportManagerV1;

const FRAME: Duration = Duration::from_micros(1_000_000 / 30);

type Done = Box<dyn FnOnce(Option<gdk::Texture>)>;

struct Layout {
    format: gdk::MemoryFormat,
    code: wl_shm::Format,
    width: u32,
    height: u32,
    stride: u32,
}

struct Pending {
    frame: HyprlandToplevelExportFrameV1,
    limit: (u32, u32),
    ignore_damage: bool,
    layout: Option<Layout>,
    offer: Option<(u32, u32, u32)>,
    file: Option<File>,
    pool: Option<WlShmPool>,
    params: Option<ZwpLinuxBufferParamsV1>,
    dmabuf: Option<gbm::Buffer>,
    buffer: Option<WlBuffer>,
    inverted: bool,
    finished: Option<bool>,
    done: Option<Done>,
}

struct State {
    shm: WlShm,
    linux_dmabuf: Option<ZwpLinuxDmabufV1>,
    main_device: Option<u64>,
    table: Vec<(u32, u64)>,
    accepted: Vec<(u32, u64)>,
    gbm: Option<Gbm>,
    modifiers: Vec<(u32, u64)>,
    broken: bool,
    pending: HashMap<u64, Pending>,
}

impl Dispatch<HyprlandToplevelExportFrameV1, u64> for State {
    fn event(
        state: &mut State,
        _: &HyprlandToplevelExportFrameV1,
        event: hyprland_toplevel_export_frame_v1::Event,
        id: &u64,
        _: &Connection,
        queue: &QueueHandle<State>,
    ) {
        let State {
            shm,
            linux_dmabuf,
            gbm,
            modifiers,
            broken,
            pending,
            ..
        } = state;
        let Some(pending) = pending.get_mut(id) else {
            return;
        };
        match event {
            hyprland_toplevel_export_frame_v1::Event::Buffer {
                format: WEnum::Value(code),
                width,
                height,
                stride,
            } => {
                if pending.layout.is_none()
                    && let Some(format) = memory_format(code)
                {
                    pending.layout = Some(Layout {
                        format,
                        code,
                        width,
                        height,
                        stride,
                    });
                }
            }
            hyprland_toplevel_export_frame_v1::Event::LinuxDmabuf {
                format,
                width,
                height,
            } => {
                if pending.offer.is_none() {
                    pending.offer = Some((format, width, height));
                }
            }
            hyprland_toplevel_export_frame_v1::Event::BufferDone => {
                if let (Some((fourcc, width, height)), Some(linux_dmabuf), Some(gbm), false) =
                    (pending.offer, linux_dmabuf.as_ref(), gbm.as_ref(), *broken)
                {
                    let wanted: Vec<u64> = modifiers
                        .iter()
                        .filter(|(format, _)| *format == fourcc)
                        .map(|(_, modifier)| *modifier)
                        .collect();
                    if let Some(buffer) = gbm.allocate(width, height, fourcc, &wanted) {
                        let params = linux_dmabuf.create_params(queue, *id);
                        for (index, plane) in buffer.planes.iter().enumerate() {
                            params.add(
                                plane.fd.as_fd(),
                                index as u32,
                                plane.offset,
                                plane.stride,
                                (buffer.modifier >> 32) as u32,
                                buffer.modifier as u32,
                            );
                        }
                        params.create(
                            width as i32,
                            height as i32,
                            fourcc,
                            zwp_linux_buffer_params_v1::Flags::empty(),
                        );
                        pending.params = Some(params);
                        pending.dmabuf = Some(buffer);
                        return;
                    }
                }
                copy_shm(shm, pending, *id, queue);
            }
            hyprland_toplevel_export_frame_v1::Event::Flags { flags } => {
                pending.inverted = match flags {
                    WEnum::Value(hyprland_toplevel_export_frame_v1::Flags::YInvert) => true,
                    WEnum::Unknown(bits) => bits & 1 == 1,
                };
            }
            hyprland_toplevel_export_frame_v1::Event::Ready { .. } => {
                pending.finished = Some(true);
            }
            hyprland_toplevel_export_frame_v1::Event::Failed => {
                if pending.dmabuf.is_some() {
                    *broken = true;
                }
                pending.finished = Some(false);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, u64> for State {
    fn event(
        state: &mut State,
        params: &ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        id: &u64,
        _: &Connection,
        queue: &QueueHandle<State>,
    ) {
        params.destroy();
        let State {
            shm,
            broken,
            pending,
            ..
        } = state;
        let Some(pending) = pending.get_mut(id) else {
            return;
        };
        pending.params = None;
        match event {
            zwp_linux_buffer_params_v1::Event::Created { buffer } => {
                pending.frame.copy(&buffer, pending.ignore_damage as i32);
                pending.buffer = Some(buffer);
            }
            zwp_linux_buffer_params_v1::Event::Failed => {
                *broken = true;
                pending.dmabuf = None;
                copy_shm(shm, pending, *id, queue);
            }
        }
    }

    event_created_child!(State, ZwpLinuxBufferParamsV1, [
        zwp_linux_buffer_params_v1::EVT_CREATED_OPCODE => (WlBuffer, ()),
    ]);
}

impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for State {
    fn event(
        state: &mut State,
        _: &ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        match event {
            zwp_linux_dmabuf_feedback_v1::Event::MainDevice { device } => {
                if let Ok(bytes) = <[u8; 8]>::try_from(device.as_slice()) {
                    state.main_device = Some(u64::from_ne_bytes(bytes));
                }
            }
            zwp_linux_dmabuf_feedback_v1::Event::FormatTable { fd, size } => {
                let mut bytes = vec![0u8; size as usize];
                if File::from(fd).read_exact_at(&mut bytes, 0).is_ok() {
                    state.table = bytes
                        .chunks_exact(16)
                        .map(|entry| {
                            (
                                u32::from_ne_bytes(entry[..4].try_into().unwrap()),
                                u64::from_ne_bytes(entry[8..].try_into().unwrap()),
                            )
                        })
                        .collect();
                }
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheFormats { indices } => {
                for index in indices.chunks_exact(2) {
                    let index = u16::from_ne_bytes([index[0], index[1]]) as usize;
                    if let Some(pair) = state.table.get(index)
                        && !state.accepted.contains(pair)
                    {
                        state.accepted.push(*pair);
                    }
                }
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore HyprlandToplevelExportManagerV1);
delegate_noop!(State: ignore ZwpLinuxDmabufV1);
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: ignore WlShmPool);
delegate_noop!(State: ignore WlBuffer);

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut State,
        _: &WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

pub struct Capture {
    display: gdk::Display,
    anchor: glib::WeakRef<gtk4::Widget>,
    connection: Connection,
    queue: RefCell<wayland_client::EventQueue<State>>,
    state: RefCell<State>,
    manager: HyprlandToplevelExportManagerV1,
    next: Cell<u64>,
    pumping: Cell<bool>,
    requested: RefCell<HashMap<String, Instant>>,
    generation: Cell<u64>,
}

impl Capture {
    pub fn new(anchor: &impl IsA<gtk4::Widget>) -> Option<Rc<Self>> {
        let display = anchor.display();
        let wayland = display
            .clone()
            .downcast::<gdk4_wayland::WaylandDisplay>()
            .ok()?;
        let native = wayland.wl_display_raw()?;
        let backend = unsafe {
            wayland_backend::sys::client::Backend::from_foreign_display(native.as_ptr().cast())
        };
        let connection = Connection::from_backend(backend);
        let (globals, mut queue) = globals::registry_queue_init::<State>(&connection).ok()?;
        let manager: HyprlandToplevelExportManagerV1 =
            globals.bind(&queue.handle(), 1..=1, ()).ok()?;
        let shm: WlShm = globals.bind(&queue.handle(), 1..=1, ()).ok()?;
        let linux_dmabuf: Option<ZwpLinuxDmabufV1> = globals.bind(&queue.handle(), 4..=5, ()).ok();
        let mut state = State {
            shm,
            main_device: None,
            table: Vec::new(),
            accepted: Vec::new(),
            gbm: None,
            modifiers: Vec::new(),
            broken: false,
            pending: HashMap::new(),
            linux_dmabuf,
        };
        if let Some(linux_dmabuf) = state.linux_dmabuf.as_ref() {
            let feedback = linux_dmabuf.get_default_feedback(&queue.handle(), ());
            let _ = queue.roundtrip(&mut state);
            feedback.destroy();
            state.table = Vec::new();
            let node = state.main_device.and_then(gbm::render_node);
            let sampled = node
                .as_ref()
                .and_then(|node| std::fs::metadata(node).ok())
                .map(|metadata| vulkan::sampled_modifiers(gbm::numbers(metadata.rdev())))
                .unwrap_or_default();
            let formats = display.dmabuf_formats();
            state.modifiers = (0..formats.n_formats())
                .map(|index| formats.format(index))
                .filter(|pair| state.accepted.contains(pair) && sampled.contains(pair))
                .collect();
            state.gbm = node
                .filter(|_| !state.modifiers.is_empty())
                .and_then(|node| Gbm::open(&node));
        }
        Some(Rc::new(Capture {
            anchor: anchor.upcast_ref::<gtk4::Widget>().downgrade(),
            display,
            connection,
            queue: RefCell::new(queue),
            state: RefCell::new(state),
            manager,
            next: Cell::new(0),
            pumping: Cell::new(false),
            requested: RefCell::new(HashMap::new()),
            generation: Cell::new(0),
        }))
    }

    pub fn grab(
        self: &Rc<Self>,
        address: &str,
        limit: (u32, u32),
        done: impl FnOnce(Option<gdk::Texture>) + 'static,
    ) {
        self.request(address, limit, true, done);
    }

    pub fn next_frame(
        self: &Rc<Self>,
        address: &str,
        limit: (u32, u32),
        done: impl FnOnce(Option<gdk::Texture>) + 'static,
    ) {
        self.request(address, limit, false, done);
    }

    pub fn stop(&self) {
        let pending: Vec<Pending> = self
            .state
            .borrow_mut()
            .pending
            .drain()
            .map(|(_, pending)| pending)
            .collect();
        for mut pending in pending {
            release(&mut pending);
        }
        self.requested.borrow_mut().clear();
        self.generation.set(self.generation.get() + 1);
        let _ = self.connection.flush();
    }

    fn request(
        self: &Rc<Self>,
        address: &str,
        limit: (u32, u32),
        ignore_damage: bool,
        done: impl FnOnce(Option<gdk::Texture>) + 'static,
    ) {
        let Ok(handle) = u64::from_str_radix(address.trim_start_matches("0x"), 16) else {
            done(None);
            return;
        };
        let wait = self
            .requested
            .borrow()
            .get(address)
            .filter(|_| !ignore_damage)
            .map(|last| FRAME.saturating_sub(last.elapsed()))
            .unwrap_or_default();
        if !wait.is_zero() {
            let capture = self.clone();
            let address = address.to_owned();
            let generation = self.generation.get();
            glib::timeout_add_local_once(wait, move || {
                if capture.generation.get() == generation {
                    capture.request(&address, limit, ignore_damage, done);
                }
            });
            return;
        }
        self.requested
            .borrow_mut()
            .insert(address.to_owned(), Instant::now());
        let id = self.next.get();
        self.next.set(id + 1);
        let frame = {
            let queue = self.queue.borrow();
            self.manager
                .capture_toplevel(0, handle as u32, &queue.handle(), id)
        };
        self.state.borrow_mut().pending.insert(
            id,
            Pending {
                frame,
                limit,
                ignore_damage,
                layout: None,
                offer: None,
                file: None,
                pool: None,
                params: None,
                dmabuf: None,
                buffer: None,
                inverted: false,
                finished: None,
                done: Some(Box::new(done)),
            },
        );
        let _ = self.connection.flush();
        if self.pumping.replace(true) {
            return;
        }
        let capture = self.clone();
        let fd = self.connection.backend().poll_fd().as_raw_fd();
        readable::when_readable(fd, move || {
            capture.pump();
            if capture.state.borrow().pending.is_empty() {
                capture.pumping.set(false);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    fn pump(&self) {
        {
            let mut queue = self.queue.borrow_mut();
            let mut state = self.state.borrow_mut();
            let _ = queue.dispatch_pending(&mut state);
            if let Some(guard) = queue.prepare_read() {
                let _ = guard.read();
            }
            let _ = queue.dispatch_pending(&mut state);
        }
        let _ = self.connection.flush();
        let finished: Vec<u64> = self
            .state
            .borrow()
            .pending
            .iter()
            .filter(|(_, pending)| pending.finished.is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in finished {
            let Some(mut pending) = self.state.borrow_mut().pending.remove(&id) else {
                continue;
            };
            let texture = if pending.finished == Some(true) {
                self.texture(&mut pending)
            } else {
                None
            };
            release(&mut pending);
            if let Some(done) = pending.done.take() {
                done(texture);
            }
        }
        let _ = self.connection.flush();
    }

    fn texture(&self, pending: &mut Pending) -> Option<gdk::Texture> {
        let Some(buffer) = pending.dmabuf.take() else {
            return shm_texture(pending);
        };
        let size = (buffer.width as usize, buffer.height as usize);
        let texture = match dmabuf_texture(&self.display, buffer) {
            Ok(texture) => texture,
            Err(error) => {
                eprintln!("capture: dmabuf import failed: {error}");
                self.state.borrow_mut().broken = true;
                return None;
            }
        };
        let Some(renderer) = self
            .anchor
            .upgrade()
            .and_then(|anchor| anchor.native())
            .and_then(|native| native.renderer())
        else {
            return Some(texture);
        };
        let (width, height) = fit(size.0, size.1, pending.limit);
        let bounds = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
        let scaled = gsk::TextureScaleNode::new(&texture, &bounds, gsk::ScalingFilter::Trilinear);
        let node: gsk::RenderNode = if pending.inverted {
            let flip = gsk::Transform::new()
                .translate(&graphene::Point::new(0.0, height as f32))
                .scale(1.0, -1.0);
            gsk::TransformNode::new(scaled, Some(&flip)).upcast()
        } else {
            scaled.upcast()
        };
        Some(renderer.render_texture(&node, Some(&bounds)))
    }
}

fn copy_shm(shm: &WlShm, pending: &mut Pending, id: u64, queue: &QueueHandle<State>) {
    let Some(layout) = pending
        .layout
        .as_ref()
        .filter(|layout| layout.width > 0 && layout.stride * layout.height > 0)
    else {
        pending.finished = Some(false);
        return;
    };
    let size = (layout.stride * layout.height) as i32;
    let Some(file) = scratch_file(id, size as u64) else {
        pending.finished = Some(false);
        return;
    };
    let pool = shm.create_pool(file.as_fd(), size, queue, ());
    let buffer = pool.create_buffer(
        0,
        layout.width as i32,
        layout.height as i32,
        layout.stride as i32,
        layout.code,
        queue,
        (),
    );
    pending.frame.copy(&buffer, pending.ignore_damage as i32);
    pending.file = Some(file);
    pending.pool = Some(pool);
    pending.buffer = Some(buffer);
}

fn release(pending: &mut Pending) {
    pending.frame.destroy();
    if let Some(params) = pending.params.take() {
        params.destroy();
    }
    if let Some(buffer) = pending.buffer.take() {
        buffer.destroy();
    }
    if let Some(pool) = pending.pool.take() {
        pool.destroy();
    }
}

fn memory_format(code: wl_shm::Format) -> Option<gdk::MemoryFormat> {
    match code {
        wl_shm::Format::Argb8888 => Some(gdk::MemoryFormat::B8g8r8a8Premultiplied),
        wl_shm::Format::Xrgb8888 => Some(gdk::MemoryFormat::B8g8r8x8),
        wl_shm::Format::Abgr8888 => Some(gdk::MemoryFormat::R8g8b8a8Premultiplied),
        wl_shm::Format::Xbgr8888 => Some(gdk::MemoryFormat::R8g8b8x8),
        _ => None,
    }
}

fn scratch_file(id: u64, size: u64) -> Option<File> {
    let path =
        glib::user_runtime_dir().join(format!("proscenio-capture-{}-{id}", std::process::id()));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .ok()?;
    let _ = std::fs::remove_file(&path);
    file.set_len(size).ok()?;
    Some(file)
}

fn dmabuf_texture(
    display: &gdk::Display,
    buffer: gbm::Buffer,
) -> Result<gdk::Texture, glib::Error> {
    let mut builder = gdk::DmabufTextureBuilder::new()
        .set_display(display)
        .set_width(buffer.width)
        .set_height(buffer.height)
        .set_fourcc(buffer.fourcc)
        .set_modifier(buffer.modifier)
        .set_n_planes(buffer.planes.len() as u32);
    for (index, plane) in buffer.planes.iter().enumerate() {
        let index = index as u32;
        builder = unsafe { builder.set_fd(index, plane.fd.as_raw_fd()) }
            .set_stride(index, plane.stride)
            .set_offset(index, plane.offset);
    }
    unsafe { builder.build_with_release_func(move || drop(buffer)) }
}

fn shm_texture(pending: &Pending) -> Option<gdk::Texture> {
    let layout = pending.layout.as_ref()?;
    let file = pending.file.as_ref()?;
    let stride = layout.stride as usize;
    let size = (layout.width as usize, layout.height as usize);
    let mut bytes = vec![0u8; stride * size.1];
    file.read_exact_at(&mut bytes, 0).ok()?;
    Some(shrunk_texture(
        &bytes,
        stride,
        size,
        layout.format,
        pending.limit,
        pending.inverted,
    ))
}

fn shrunk_texture(
    bytes: &[u8],
    stride: usize,
    (width, height): (usize, usize),
    format: gdk::MemoryFormat,
    limit: (u32, u32),
    inverted: bool,
) -> gdk::Texture {
    let (target_width, target_height) = fit(width, height, limit);
    let shrunk = shrink(
        bytes,
        stride,
        (width, height),
        (target_width, target_height),
        inverted,
    );
    gdk::MemoryTexture::new(
        target_width as i32,
        target_height as i32,
        format,
        &glib::Bytes::from_owned(shrunk),
        target_width * 4,
    )
    .upcast()
}

fn fit(width: usize, height: usize, (max_width, max_height): (u32, u32)) -> (usize, usize) {
    let scale = (max_width as f64 / width as f64)
        .min(max_height as f64 / height as f64)
        .min(1.0);
    (
        ((width as f64 * scale).round() as usize).max(1),
        ((height as f64 * scale).round() as usize).max(1),
    )
}

fn shrink(
    bytes: &[u8],
    stride: usize,
    (width, height): (usize, usize),
    (target_width, target_height): (usize, usize),
    inverted: bool,
) -> Vec<u8> {
    let mut out = vec![0u8; target_width * target_height * 4];
    for target_y in 0..target_height {
        let top = target_y * height / target_height;
        let bottom = ((target_y + 1) * height / target_height).max(top + 1);
        for target_x in 0..target_width {
            let left = target_x * width / target_width;
            let right = ((target_x + 1) * width / target_width).max(left + 1);
            let mut sum = [0u32; 4];
            for y in top..bottom {
                let row = if inverted { height - 1 - y } else { y };
                let pixels = &bytes[row * stride + left * 4..row * stride + right * 4];
                for pixel in pixels.chunks_exact(4) {
                    for (total, channel) in sum.iter_mut().zip(pixel) {
                        *total += *channel as u32;
                    }
                }
            }
            let count = ((bottom - top) * (right - left)) as u32;
            let at = (target_y * target_width + target_x) * 4;
            for (slot, total) in out[at..at + 4].iter_mut().zip(sum) {
                *slot = ((total + count / 2) / count) as u8;
            }
        }
    }
    out
}
