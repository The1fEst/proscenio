use proscenio_protocol::{Event, Message};
use std::io::Write;

pub fn emit(event: &Event) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(&event.encode());
    let _ = stdout.flush();
}

pub fn finish(result: Result<(), String>) -> i32 {
    let is_ok = result.is_ok();
    emit(&Event::Done {
        is_ok,
        message: result.err().unwrap_or_default(),
    });
    if is_ok { 0 } else { 1 }
}
