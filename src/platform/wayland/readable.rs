use gtk4::glib;
use gtk4::glib::translate::{FromGlib, IntoGlib};
use std::ffi::{c_int, c_uint, c_void};
use std::io::{ErrorKind, Read};
use std::os::fd::{AsRawFd, RawFd};
use wayland_client::{Connection, EventQueue};

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

pub fn add_watch(
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

pub fn add_reader(
    mut reader: impl Read + AsRawFd + 'static,
    mut handler: impl FnMut(Option<&[u8]>) + 'static,
) -> glib::SourceId {
    let fd = reader.as_raw_fd();
    let mut chunk = [0u8; 4096];
    add_watch(fd, move || {
        read_chunk(&mut reader, &mut chunk, &mut handler)
    })
}

fn read_chunk(
    reader: &mut impl Read,
    chunk: &mut [u8],
    handler: &mut impl FnMut(Option<&[u8]>),
) -> glib::ControlFlow {
    loop {
        match reader.read(chunk) {
            Ok(0) => break,
            Ok(length) => {
                handler(Some(&chunk[..length]));
                return glib::ControlFlow::Continue;
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                return glib::ControlFlow::Continue;
            }
            Err(_) => break,
        }
    }
    handler(None);
    glib::ControlFlow::Break
}

pub fn pump<State>(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
) -> glib::ControlFlow {
    let _ = queue.dispatch_pending(state);
    if let Some(guard) = queue.prepare_read() {
        let _ = guard.read();
    }
    let _ = queue.dispatch_pending(state);
    match connection.backend().last_error() {
        Some(_) => glib::ControlFlow::Break,
        None => glib::ControlFlow::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use wayland_client::delegate_noop;
    use wayland_client::protocol::wl_callback::WlCallback;

    struct NullState;

    delegate_noop!(NullState: ignore WlCallback);

    fn to_bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_ne_bytes()).collect()
    }

    struct FakeInterruptedReader {
        is_interrupted: bool,
        source: UnixStream,
    }

    impl Read for FakeInterruptedReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.is_interrupted {
                self.is_interrupted = false;
                return Err(ErrorKind::Interrupted.into());
            }
            self.source.read(buffer)
        }
    }

    fn read_once(reader: &mut impl Read) -> (glib::ControlFlow, Vec<Option<Vec<u8>>>) {
        let mut chunks = Vec::new();
        let flow = read_chunk(reader, &mut [0u8; 64], &mut |bytes: Option<&[u8]>| {
            chunks.push(bytes.map(<[u8]>::to_vec))
        });
        (flow, chunks)
    }

    #[test]
    fn an_interrupted_read_is_retried() {
        let (source, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(b"last line\n").unwrap();
        drop(writer);
        let mut reader = FakeInterruptedReader {
            is_interrupted: true,
            source,
        };
        assert_eq!(
            read_once(&mut reader),
            (
                glib::ControlFlow::Continue,
                vec![Some(b"last line\n".to_vec())]
            )
        );
        assert_eq!(
            read_once(&mut reader),
            (glib::ControlFlow::Break, vec![None])
        );
    }

    #[test]
    fn an_empty_nonblocking_reader_waits_for_the_next_readable_event() {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        assert_eq!(
            read_once(&mut reader),
            (glib::ControlFlow::Continue, vec![])
        );
        writer.write_all(b"progress").unwrap();
        drop(writer);
        assert_eq!(
            read_once(&mut reader),
            (
                glib::ControlFlow::Continue,
                vec![Some(b"progress".to_vec())]
            )
        );
        assert_eq!(
            read_once(&mut reader),
            (glib::ControlFlow::Break, vec![None])
        );
    }

    #[test]
    fn pump_keeps_stopping_after_the_compositor_posts_an_error() {
        let (client, mut compositor) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let mut queue = connection.new_event_queue::<NullState>();
        connection.display().sync(&queue.handle(), ());
        let callback_done = [2, 12 << 16, 0];
        let display_error = [1, 24 << 16, 1, 0, 2, u32::from_ne_bytes(*b"x\0\0\0")];
        compositor
            .write_all(&to_bytes(
                &[callback_done.as_slice(), &display_error].concat(),
            ))
            .unwrap();
        assert_eq!(
            pump(&connection, &mut queue, &mut NullState),
            glib::ControlFlow::Break
        );
        assert_eq!(
            pump(&connection, &mut queue, &mut NullState),
            glib::ControlFlow::Break
        );
    }

    #[test]
    fn pump_stops_once_the_compositor_hangs_up() {
        let (client, compositor) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let mut queue = connection.new_event_queue::<()>();
        assert_eq!(
            pump(&connection, &mut queue, &mut ()),
            glib::ControlFlow::Continue
        );
        drop(compositor);
        assert_eq!(
            pump(&connection, &mut queue, &mut ()),
            glib::ControlFlow::Break
        );
    }
}
