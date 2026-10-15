use gdk4_wayland::prelude::*;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::Cast;
use std::cell::RefCell;
use std::os::fd::AsRawFd;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop, globals};

use crate::platform::wayland::readable;

pub mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-focus-grab-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-focus-grab-v1.xml");
}

use protocol::hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1;
use protocol::hyprland_focus_grab_v1::{self, HyprlandFocusGrabV1};

struct State;

impl Dispatch<HyprlandFocusGrabV1, Arc<AtomicBool>> for State {
    fn event(
        _: &mut State,
        _: &HyprlandFocusGrabV1,
        event: hyprland_focus_grab_v1::Event,
        is_cleared: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if matches!(event, hyprland_focus_grab_v1::Event::Cleared) {
            is_cleared.store(true, Ordering::Relaxed);
        }
    }
}

delegate_noop!(State: ignore HyprlandFocusGrabManagerV1);

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

thread_local! {
    static PERSISTENT_SURFACES: RefCell<Vec<glib::WeakRef<gdk::Surface>>> = const { RefCell::new(Vec::new()) };
    static GRABS: RefCell<Vec<Weak<Grab>>> = const { RefCell::new(Vec::new()) };
    static SHARED: RefCell<Option<Rc<Shared>>> = const { RefCell::new(None) };
}

pub fn add_persistent(surface: &gdk::Surface) {
    PERSISTENT_SURFACES.with(|list| list.borrow_mut().push(surface.downgrade()));
    for grab in grabs() {
        grab.change(surface, true);
    }
}

pub fn remove_persistent(surface: &gdk::Surface) {
    PERSISTENT_SURFACES.with(|list| {
        list.borrow_mut()
            .retain(|weak| weak.upgrade().is_some_and(|other| other != *surface));
    });
    for grab in grabs() {
        grab.change(surface, false);
    }
}

fn persistent_surfaces() -> Vec<gdk::Surface> {
    PERSISTENT_SURFACES.with(|list| {
        list.borrow()
            .iter()
            .filter_map(|weak| weak.upgrade())
            .collect()
    })
}

fn grabs() -> Vec<Rc<Grab>> {
    GRABS.with(|list| {
        let mut list = list.borrow_mut();
        list.retain(|grab| grab.strong_count() > 0);
        list.iter().filter_map(Weak::upgrade).collect()
    })
}

fn wl_surface(surface: &gdk::Surface) -> Option<WlSurface> {
    surface
        .clone()
        .downcast::<gdk4_wayland::WaylandSurface>()
        .ok()?
        .wl_surface()
}

struct Shared {
    connection: Connection,
    queue: RefCell<EventQueue<State>>,
    manager: HyprlandFocusGrabManagerV1,
    source_id: RefCell<Option<glib::SourceId>>,
}

fn shared(display: &gdk::Display) -> Option<Rc<Shared>> {
    if let Some(shared) = SHARED.with_borrow(Clone::clone) {
        return Some(shared);
    }
    let display = display
        .clone()
        .downcast::<gdk4_wayland::WaylandDisplay>()
        .ok()?;
    let native = display.wl_display_raw()?;
    let backend = unsafe {
        wayland_backend::sys::client::Backend::from_foreign_display(native.as_ptr().cast())
    };
    let shared = Rc::new(Shared::new(Connection::from_backend(backend))?);
    SHARED.with_borrow_mut(|slot| *slot = Some(shared.clone()));
    Some(shared)
}

impl Shared {
    fn new(connection: Connection) -> Option<Self> {
        let (globals, queue) = globals::registry_queue_init::<State>(&connection).ok()?;
        let manager: HyprlandFocusGrabManagerV1 = globals.bind(&queue.handle(), 1..=1, ()).ok()?;
        Some(Shared {
            connection,
            queue: RefCell::new(queue),
            manager,
            source_id: RefCell::new(None),
        })
    }

    fn flush(&self) {
        let _ = self.connection.flush();
    }

    fn pump(&self) {
        let mut queue = self.queue.borrow_mut();
        let _ = queue.dispatch_pending(&mut State);
        if let Some(guard) = queue.prepare_read() {
            let _ = guard.read();
        }
        let _ = queue.dispatch_pending(&mut State);
    }

    fn start_watch(self: &Rc<Self>) {
        if self.source_id.borrow().is_some() {
            return;
        }
        let shared = Rc::downgrade(self);
        let fd = self.connection.backend().poll_fd().as_raw_fd();
        let source_id = readable::add_watch(fd, move || {
            let Some(shared) = shared.upgrade() else {
                return glib::ControlFlow::Break;
            };
            shared.pump();
            for grab in grabs() {
                grab.release_if_cleared();
            }
            if grabs()
                .iter()
                .any(|grab| grab.active_grab.borrow().is_some())
            {
                return glib::ControlFlow::Continue;
            }
            shared.source_id.take();
            glib::ControlFlow::Break
        });
        self.source_id.replace(Some(source_id));
    }
}

pub struct Grab {
    shared: Rc<Shared>,
    is_cleared: Arc<AtomicBool>,
    on_cleared: RefCell<Option<Box<dyn Fn()>>>,
    active_grab: RefCell<Option<HyprlandFocusGrabV1>>,
}

impl Grab {
    pub fn new(display: &gdk::Display) -> Option<Rc<Self>> {
        let grab = Rc::new(Grab {
            shared: shared(display)?,
            is_cleared: Arc::new(AtomicBool::new(false)),
            on_cleared: RefCell::new(None),
            active_grab: RefCell::new(None),
        });
        GRABS.with(|list| list.borrow_mut().push(Rc::downgrade(&grab)));
        Some(grab)
    }

    pub fn hold(self: &Rc<Self>, surface: &gdk::Surface, on_cleared: impl Fn() + 'static) {
        self.release();
        let Some(surface) = wl_surface(surface) else {
            return;
        };

        self.is_cleared.store(false, Ordering::Relaxed);
        let queue = self.shared.queue.borrow();
        let grab = self
            .shared
            .manager
            .create_grab(&queue.handle(), self.is_cleared.clone());
        grab.add_surface(&surface);
        for persistent_surface in persistent_surfaces().iter().filter_map(wl_surface) {
            grab.add_surface(&persistent_surface);
        }
        grab.commit();
        drop(queue);
        self.shared.flush();
        self.active_grab.replace(Some(grab));
        self.on_cleared.replace(Some(Box::new(on_cleared)));
        self.shared.start_watch();
    }

    pub fn release(&self) {
        self.on_cleared.take();
        self.destroy_active_grab();
    }

    fn release_if_cleared(&self) {
        if !self.is_cleared.swap(false, Ordering::Relaxed) {
            return;
        }
        self.destroy_active_grab();
        if let Some(on_cleared) = self.on_cleared.take() {
            on_cleared();
        }
    }

    fn destroy_active_grab(&self) {
        if let Some(grab) = self.active_grab.borrow_mut().take() {
            grab.destroy();
            self.shared.flush();
        }
    }

    fn change(&self, surface: &gdk::Surface, should_add: bool) {
        let (Some(grab), Some(surface)) = (self.active_grab.borrow().clone(), wl_surface(surface))
        else {
            return;
        };
        if should_add {
            grab.add_surface(&surface);
        } else {
            grab.remove_surface(&surface);
        }
        grab.commit();
        self.shared.flush();
    }
}

impl Drop for Grab {
    fn drop(&mut self) {
        self.destroy_active_grab();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use wayland_client::Proxy;
    use wayland_client::protocol::{wl_callback, wl_registry};

    fn to_bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_ne_bytes()).collect()
    }

    fn connect() -> (Rc<Shared>, UnixStream) {
        let (client, mut compositor) = UnixStream::pair().unwrap();
        let interface = b"hyprland_focus_grab_manager_v1\0\0";
        let registry_global = [
            to_bytes(&[
                2,
                52 << 16 | u32::from(wl_registry::EVT_GLOBAL_OPCODE),
                1,
                31,
            ]),
            interface.to_vec(),
            to_bytes(&[1]),
        ]
        .concat();
        let callback_done = to_bytes(&[3, 12 << 16 | u32::from(wl_callback::EVT_DONE_OPCODE), 0]);
        compositor
            .write_all(&[registry_global, callback_done].concat())
            .unwrap();
        let shared = Shared::new(Connection::from_socket(client).unwrap()).unwrap();
        (Rc::new(shared), compositor)
    }

    fn read_requests(compositor: &mut UnixStream) -> Vec<(u32, u32)> {
        let mut bytes = Vec::new();
        compositor.set_nonblocking(true).unwrap();
        let _ = compositor.read_to_end(&mut bytes);
        let words: Vec<u32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| u32::from_ne_bytes(*chunk))
            .collect();
        let mut requests = Vec::new();
        let mut index = 0;
        while index + 1 < words.len() {
            requests.push((words[index], words[index + 1] & 0xffff));
            index += ((words[index + 1] >> 16) as usize / 4).max(2);
        }
        requests
    }

    #[test]
    fn every_way_of_letting_go_of_a_grab_destroys_it() {
        let actions: [fn(Grab) -> Option<Grab>; 3] = [
            |grab| {
                grab.release();
                Some(grab)
            },
            |grab| {
                grab.is_cleared.store(true, Ordering::Relaxed);
                grab.release_if_cleared();
                Some(grab)
            },
            |grab| {
                drop(grab);
                None
            },
        ];
        for (index, action) in actions.into_iter().enumerate() {
            let (shared, mut compositor) = connect();
            let is_cleared = Arc::new(AtomicBool::new(false));
            let active_grab = shared
                .manager
                .create_grab(&shared.queue.borrow().handle(), is_cleared.clone());
            let id = active_grab.id().protocol_id();
            let remaining_grab = action(Grab {
                shared: shared.clone(),
                is_cleared,
                on_cleared: RefCell::new(None),
                active_grab: RefCell::new(Some(active_grab)),
            });
            shared.flush();
            let destroy_request = (id, u32::from(hyprland_focus_grab_v1::REQ_DESTROY_OPCODE));
            assert!(
                read_requests(&mut compositor).contains(&destroy_request),
                "action {index}"
            );
            drop(remaining_grab);
        }
    }
}
