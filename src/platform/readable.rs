use gtk4::glib;
use gtk4::glib::translate::{FromGlib, IntoGlib};
use std::ffi::{c_int, c_uint, c_void};
use std::os::fd::RawFd;

type Callback = Box<dyn FnMut() -> glib::ControlFlow>;

unsafe extern "C" {
    fn g_unix_fd_add_full(
        priority: c_int,
        fd: c_int,
        condition: c_uint,
        function: unsafe extern "C" fn(c_int, c_uint, *mut c_void) -> c_int,
        data: *mut c_void,
        notify: unsafe extern "C" fn(*mut c_void),
    ) -> c_uint;
}

unsafe extern "C" fn trampoline(_: c_int, _: c_uint, data: *mut c_void) -> c_int {
    let callback = unsafe { &mut *data.cast::<Callback>() };
    callback().into_glib()
}

unsafe extern "C" fn release(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<Callback>()) });
}

pub fn when_readable(
    fd: RawFd,
    callback: impl FnMut() -> glib::ControlFlow + 'static,
) -> glib::SourceId {
    let data: *mut Callback = Box::into_raw(Box::new(Box::new(callback)));
    let id = unsafe {
        g_unix_fd_add_full(
            glib::Priority::DEFAULT.into_glib(),
            fd,
            glib::IOCondition::IN.bits(),
            trampoline,
            data.cast(),
            release,
        )
    };
    unsafe { glib::SourceId::from_glib(id) }
}
