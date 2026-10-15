use gtk4::glib;
use std::cell::RefCell;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, globals};

use crate::platform::wayland::readable;

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
    on_change: Box<dyn Fn(bool)>,
    on_disconnected: Box<dyn Fn()>,
}

impl LockNotifier {
    pub fn watch(
        on_change: impl Fn(bool) + 'static,
        on_disconnected: impl Fn() + 'static,
    ) -> Option<Rc<Self>> {
        let connection = Connection::connect_to_env().ok()?;
        let (globals, queue) = globals::registry_queue_init::<State>(&connection).ok()?;
        let notifier: HyprlandLockNotifierV1 = globals.bind(&queue.handle(), 1..=1, ()).ok()?;
        let notification = notifier.get_lock_notification(&queue.handle(), ());
        let _ = connection.flush();

        let lock_notifier = Rc::new(LockNotifier {
            connection,
            queue: RefCell::new(queue),
            state: RefCell::new(State::default()),
            notifier,
            notification,
            on_change: Box::new(on_change),
            on_disconnected: Box::new(on_disconnected),
        });

        let weak = Rc::downgrade(&lock_notifier);
        let fd = lock_notifier.connection.backend().poll_fd().as_raw_fd();
        readable::add_watch(fd, move || {
            let Some(lock_notifier) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            lock_notifier.pump()
        });
        Some(lock_notifier)
    }

    pub fn pump(&self) -> glib::ControlFlow {
        let flow = readable::pump(
            &self.connection,
            &mut self.queue.borrow_mut(),
            &mut self.state.borrow_mut(),
        );
        let changes = std::mem::take(&mut self.state.borrow_mut().changes);
        for is_locked in changes {
            (self.on_change)(is_locked);
        }
        if flow.is_break() {
            (self.on_disconnected)();
        }
        flow
    }
}

impl Drop for LockNotifier {
    fn drop(&mut self) {
        self.notification.destroy();
        self.notifier.destroy();
        let _ = self.connection.flush();
    }
}
