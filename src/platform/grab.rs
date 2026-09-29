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

use crate::platform::readable;

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
        cleared: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if matches!(event, hyprland_focus_grab_v1::Event::Cleared) {
            cleared.store(true, Ordering::Relaxed);
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
    static PERSISTENT: RefCell<Vec<glib::WeakRef<gdk::Surface>>> = const { RefCell::new(Vec::new()) };
    static GRABS: RefCell<Vec<Weak<Grab>>> = const { RefCell::new(Vec::new()) };
    static SHARED: RefCell<Option<Rc<Shared>>> = const { RefCell::new(None) };
}

pub fn add_persistent(surface: &gdk::Surface) {
    PERSISTENT.with(|list| list.borrow_mut().push(surface.downgrade()));
    for grab in grabs() {
        grab.change(surface, true);
    }
}

pub fn remove_persistent(surface: &gdk::Surface) {
    PERSISTENT.with(|list| {
        list.borrow_mut()
            .retain(|kept| kept.upgrade().is_some_and(|kept| kept != *surface));
    });
    for grab in grabs() {
        grab.change(surface, false);
    }
}

fn persistent() -> Vec<gdk::Surface> {
    PERSISTENT.with(|list| {
        list.borrow()
            .iter()
            .filter_map(|kept| kept.upgrade())
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
    watch: RefCell<Option<glib::SourceId>>,
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
    let connection = Connection::from_backend(backend);
    let (globals, queue) = globals::registry_queue_init::<State>(&connection).ok()?;
    let manager: HyprlandFocusGrabManagerV1 = globals.bind(&queue.handle(), 1..=1, ()).ok()?;
    let shared = Rc::new(Shared {
        connection,
        queue: RefCell::new(queue),
        manager,
        watch: RefCell::new(None),
    });
    SHARED.with_borrow_mut(|slot| *slot = Some(shared.clone()));
    Some(shared)
}

impl Shared {
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

    fn watch(self: &Rc<Self>) {
        if self.watch.borrow().is_some() {
            return;
        }
        let shared = Rc::downgrade(self);
        let fd = self.connection.backend().poll_fd().as_raw_fd();
        let watch = readable::when_readable(fd, move || {
            let Some(shared) = shared.upgrade() else {
                return glib::ControlFlow::Break;
            };
            shared.pump();
            for grab in grabs() {
                if grab.cleared.swap(false, Ordering::Relaxed) {
                    grab.forget();
                    if let Some(cleared) = grab.on_cleared.take() {
                        cleared();
                    }
                }
            }
            if grabs().iter().any(|grab| grab.held.borrow().is_some()) {
                return glib::ControlFlow::Continue;
            }
            shared.watch.take();
            glib::ControlFlow::Break
        });
        self.watch.replace(Some(watch));
    }
}

pub struct Grab {
    shared: Rc<Shared>,
    cleared: Arc<AtomicBool>,
    on_cleared: RefCell<Option<Box<dyn Fn()>>>,
    held: RefCell<Option<HyprlandFocusGrabV1>>,
}

impl Grab {
    pub fn new(display: &gdk::Display) -> Option<Rc<Self>> {
        let grab = Rc::new(Grab {
            shared: shared(display)?,
            cleared: Arc::new(AtomicBool::new(false)),
            on_cleared: RefCell::new(None),
            held: RefCell::new(None),
        });
        GRABS.with(|list| list.borrow_mut().push(Rc::downgrade(&grab)));
        Some(grab)
    }

    pub fn hold(self: &Rc<Self>, surface: &gdk::Surface, cleared: impl Fn() + 'static) {
        self.release();
        let Some(surface) = wl_surface(surface) else {
            return;
        };

        self.cleared.store(false, Ordering::Relaxed);
        let queue = self.shared.queue.borrow();
        let grab = self
            .shared
            .manager
            .create_grab(&queue.handle(), self.cleared.clone());
        grab.add_surface(&surface);
        for kept in persistent().iter().filter_map(wl_surface) {
            grab.add_surface(&kept);
        }
        grab.commit();
        drop(queue);
        self.shared.flush();
        self.held.replace(Some(grab));
        self.on_cleared.replace(Some(Box::new(cleared)));
        self.shared.watch();
    }

    pub fn release(&self) {
        self.on_cleared.take();
        if let Some(grab) = self.held.borrow_mut().take() {
            grab.destroy();
            self.shared.flush();
        }
    }

    fn forget(&self) {
        self.held.borrow_mut().take();
    }

    fn change(&self, surface: &gdk::Surface, add: bool) {
        let (Some(grab), Some(surface)) = (self.held.borrow().clone(), wl_surface(surface)) else {
            return;
        };
        if add {
            grab.add_surface(&surface);
        } else {
            grab.remove_surface(&surface);
        }
        grab.commit();
        self.shared.flush();
    }
}
