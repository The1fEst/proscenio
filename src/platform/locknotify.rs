use gtk4::glib;
use std::cell::RefCell;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, globals};

use crate::platform::readable;

pub mod protocol {
    use wayland_client;

    pub mod __interfaces {
        wayland_scanner::generate_interfaces!("protocols/hyprland-lock-notify-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-lock-notify-v1.xml");
}

use protocol::hyprland_lock_notification_v1::{self, HyprlandLockNotificationV1};
use protocol::hyprland_lock_notifier_v1::HyprlandLockNotifierV1;

#[derive(Default)]
struct State {
    changes: Vec<bool>,
}

impl Dispatch<HyprlandLockNotificationV1, ()> for State {
    fn event(
        state: &mut State,
        _: &HyprlandLockNotificationV1,
        event: hyprland_lock_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        match event {
            hyprland_lock_notification_v1::Event::Locked => state.changes.push(true),
            hyprland_lock_notification_v1::Event::Unlocked => state.changes.push(false),
        }
    }
}

delegate_noop!(State: ignore HyprlandLockNotifierV1);

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

pub struct LockNotifier {
    connection: Connection,
    queue: RefCell<wayland_client::EventQueue<State>>,
    state: RefCell<State>,
    notifier: HyprlandLockNotifierV1,
    notification: HyprlandLockNotificationV1,
    action: Box<dyn Fn(bool)>,
}

impl LockNotifier {
    pub fn watch(action: impl Fn(bool) + 'static) -> Option<Rc<Self>> {
        let connection = Connection::connect_to_env().ok()?;
        let (globals, queue) = globals::registry_queue_init::<State>(&connection).ok()?;
        let notifier: HyprlandLockNotifierV1 = globals.bind(&queue.handle(), 1..=1, ()).ok()?;
        let notification = notifier.get_lock_notification(&queue.handle(), ());
        let _ = connection.flush();

        let watcher = Rc::new(LockNotifier {
            connection,
            queue: RefCell::new(queue),
            state: RefCell::new(State::default()),
            notifier,
            notification,
            action: Box::new(action),
        });

        let weak = Rc::downgrade(&watcher);
        let fd = watcher.connection.backend().poll_fd().as_raw_fd();
        readable::when_readable(fd, move || {
            let Some(watcher) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            watcher.pump();
            glib::ControlFlow::Continue
        });
        Some(watcher)
    }

    pub fn pump(&self) {
        {
            let mut queue = self.queue.borrow_mut();
            let mut state = self.state.borrow_mut();
            let _ = queue.dispatch_pending(&mut state);
            if let Some(guard) = queue.prepare_read() {
                let _ = guard.read();
            }
            let _ = queue.dispatch_pending(&mut state);
        }
        let changes = std::mem::take(&mut self.state.borrow_mut().changes);
        for locked in changes {
            (self.action)(locked);
        }
    }
}

impl Drop for LockNotifier {
    fn drop(&mut self) {
        self.notification.destroy();
        self.notifier.destroy();
        let _ = self.connection.flush();
    }
}
