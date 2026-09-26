use gdk4_wayland::prelude::*;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::Cast;
use std::cell::RefCell;
use std::os::fd::AsRawFd;
use std::rc::{Rc, Weak};
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, globals};

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

#[derive(Default)]
struct Cleared(Rc<RefCell<bool>>);

impl Dispatch<HyprlandFocusGrabV1, ()> for State {
    fn event(
        state: &mut State,
        _: &HyprlandFocusGrabV1,
        event: hyprland_focus_grab_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if matches!(event, hyprland_focus_grab_v1::Event::Cleared) {
            state.cleared.0.replace(true);
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

struct State {
    cleared: Cleared,
}

thread_local! {
    static PERSISTENT: RefCell<Vec<glib::WeakRef<gdk::Surface>>> = const { RefCell::new(Vec::new()) };
    static GRABS: RefCell<Vec<Weak<Grab>>> = const { RefCell::new(Vec::new()) };
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

pub struct Grab {
    connection: Connection,
    queue: RefCell<wayland_client::EventQueue<State>>,
    state: RefCell<State>,
    manager: HyprlandFocusGrabManagerV1,
    held: RefCell<Option<HyprlandFocusGrabV1>>,
    watch: RefCell<Option<glib::SourceId>>,
}

impl Grab {
    pub fn new(display: &gdk::Display) -> Option<Rc<Self>> {
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

        let grab = Rc::new(Grab {
            connection,
            queue: RefCell::new(queue),
            state: RefCell::new(State {
                cleared: Cleared::default(),
            }),
            manager,
            held: RefCell::new(None),
            watch: RefCell::new(None),
        });
        GRABS.with(|list| list.borrow_mut().push(Rc::downgrade(&grab)));
        Some(grab)
    }

    pub fn hold(self: &Rc<Self>, surface: &gdk::Surface, cleared: impl Fn() + 'static) {
        self.release();
        let Some(surface) = wl_surface(surface) else {
            return;
        };

        let queue = self.queue.borrow();
        let grab = self.manager.create_grab(&queue.handle(), ());
        grab.add_surface(&surface);
        for kept in persistent().iter().filter_map(wl_surface) {
            grab.add_surface(&kept);
        }
        grab.commit();
        drop(queue);
        let _ = self.connection.flush();
        self.held.replace(Some(grab));

        let flag = self.state.borrow().cleared.0.clone();
        flag.replace(false);
        let grabber = self.clone();
        let fd = self.connection.backend().poll_fd().as_raw_fd();
        let watch = readable::when_readable(fd, move || {
            grabber.pump();
            if !flag.replace(false) {
                return glib::ControlFlow::Continue;
            }
            grabber.watch.take();
            grabber.forget();
            cleared();
            glib::ControlFlow::Break
        });
        self.watch.replace(Some(watch));
    }

    pub fn release(&self) {
        if let Some(watch) = self.watch.take() {
            watch.remove();
        }
        if let Some(grab) = self.held.borrow_mut().take() {
            grab.destroy();
            let _ = self.connection.flush();
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
        let _ = self.connection.flush();
    }

    fn pump(&self) {
        let mut queue = self.queue.borrow_mut();
        let mut state = self.state.borrow_mut();
        let _ = queue.dispatch_pending(&mut state);
        if let Some(guard) = queue.prepare_read() {
            let _ = guard.read();
        }
        let _ = queue.dispatch_pending(&mut state);
    }
}
