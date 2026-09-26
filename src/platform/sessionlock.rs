use gtk4::gdk;
use gtk4::glib;
use gtk4::glib::translate::{FromGlibPtrFull, ToGlibPtr};
use gtk4::prelude::*;

mod ffi {
    use gtk4::glib::ffi::gboolean;

    #[repr(C)]
    pub struct GtkSessionLockInstance {
        _private: [u8; 0],
    }

    #[link(name = "gtk4-layer-shell")]
    unsafe extern "C" {
        pub fn gtk_session_lock_instance_new() -> *mut GtkSessionLockInstance;
        pub fn gtk_session_lock_instance_lock(instance: *mut GtkSessionLockInstance) -> gboolean;
        pub fn gtk_session_lock_instance_unlock(instance: *mut GtkSessionLockInstance);
        pub fn gtk_session_lock_instance_assign_window_to_monitor(
            instance: *mut GtkSessionLockInstance,
            window: *mut gtk4::ffi::GtkWindow,
            monitor: *mut gtk4::gdk::ffi::GdkMonitor,
        );
    }
}

pub struct SessionLock {
    instance: glib::Object,
}

impl SessionLock {
    pub fn new() -> Self {
        let instance = unsafe {
            glib::Object::from_glib_full(
                ffi::gtk_session_lock_instance_new() as *mut glib::gobject_ffi::GObject
            )
        };
        SessionLock { instance }
    }

    fn raw(&self) -> *mut ffi::GtkSessionLockInstance {
        let pointer: *mut glib::gobject_ffi::GObject = self.instance.to_glib_none().0;
        pointer as *mut ffi::GtkSessionLockInstance
    }

    pub fn lock(&self) -> bool {
        unsafe { ffi::gtk_session_lock_instance_lock(self.raw()) != 0 }
    }

    pub fn unlock(&self) {
        unsafe { ffi::gtk_session_lock_instance_unlock(self.raw()) }
    }

    pub fn assign(&self, window: &gtk4::Window, monitor: &gdk::Monitor) {
        unsafe {
            ffi::gtk_session_lock_instance_assign_window_to_monitor(
                self.raw(),
                window.to_glib_none().0,
                monitor.to_glib_none().0,
            )
        }
    }

    pub fn connect_monitor(&self, action: impl Fn(&gdk::Monitor) + 'static) {
        self.instance
            .connect_local("monitor", false, move |values| {
                let monitor = values
                    .get(1)
                    .and_then(|value| value.get::<gdk::Monitor>().ok());
                if let Some(monitor) = monitor {
                    action(&monitor);
                }
                None
            });
    }

    pub fn connect_locked(&self, action: impl Fn() + 'static) {
        self.instance.connect_local("locked", false, move |_| {
            action();
            None
        });
    }

    pub fn connect_failed(&self, action: impl Fn() + 'static) {
        self.instance.connect_local("failed", false, move |_| {
            action();
            None
        });
    }

    pub fn connect_unlocked(&self, action: impl Fn() + 'static) {
        self.instance.connect_local("unlocked", false, move |_| {
            action();
            None
        });
    }
}
