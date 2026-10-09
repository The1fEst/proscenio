use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::os::fd::OwnedFd;
use std::rc::{Rc, Weak};

const LOGIN: &str = "org.freedesktop.login1";
const LOGIN_PATH: &str = "/org/freedesktop/login1";
const LOGIN_MANAGER: &str = "org.freedesktop.login1.Manager";

pub struct SleepWatcher {
    system: gio::DBusConnection,
    why: String,
    inhibitor: RefCell<Option<OwnedFd>>,
    _subscription: gio::SignalSubscription,
}

impl SleepWatcher {
    pub fn watch(
        system: &gio::DBusConnection,
        why: &str,
        handler: impl Fn(bool) + 'static,
    ) -> Rc<Self> {
        let watcher = Rc::new_cyclic(|weak: &Weak<Self>| {
            let weak = weak.clone();
            let subscription = system.subscribe_to_signal(
                Some(LOGIN),
                Some(LOGIN_MANAGER),
                Some("PrepareForSleep"),
                Some(LOGIN_PATH),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    let Some(sleeping) = signal.parameters.child_value(0).get::<bool>() else {
                        return;
                    };
                    if !sleeping && let Some(watcher) = weak.upgrade() {
                        watcher.inhibit();
                    }
                    handler(sleeping);
                },
            );
            SleepWatcher {
                system: system.clone(),
                why: why.to_owned(),
                inhibitor: RefCell::new(None),
                _subscription: subscription,
            }
        });
        watcher.inhibit();
        watcher
    }

    pub fn release(&self) {
        self.inhibitor.take();
    }

    fn inhibit(self: &Rc<Self>) {
        let weak: Weak<Self> = Rc::downgrade(self);
        let call = self.system.call_with_unix_fd_list_future(
            Some(LOGIN),
            LOGIN_PATH,
            LOGIN_MANAGER,
            "Inhibit",
            Some(&("sleep", "proscenio", self.why.as_str(), "delay").to_variant()),
            Some(glib::VariantTy::new("(h)").unwrap()),
            gio::DBusCallFlags::NONE,
            2000,
            None::<&gio::UnixFDList>,
        );
        glib::spawn_future_local(async move {
            let Ok((reply, Some(fds))) = call.await else {
                return;
            };
            let Some(index) = reply.child_value(0).get::<glib::variant::Handle>() else {
                return;
            };
            let Ok(fd) = fds.get(index.0) else {
                return;
            };
            if let Some(watcher) = weak.upgrade() {
                watcher.inhibitor.replace(Some(fd));
            }
        });
    }
}
