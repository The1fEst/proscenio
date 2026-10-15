use gtk4::glib;
use proscenio_protocol::{Decoder, Error, Event};
use std::cell::Cell;
use std::process::ChildStdout;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::platform::wayland::readable;

const OUTPUT_GRACE: Duration = Duration::from_secs(1);
const OUTPUT_LIMIT: Duration = Duration::from_secs(30);
const OUTPUT_CHECK_INTERVAL: Duration = Duration::from_millis(100);

pub(super) fn decode_events(
    decoder: &mut Decoder,
    bytes: Option<&[u8]>,
) -> Vec<Result<Event, Error>> {
    let Some(bytes) = bytes else {
        return match decoder.finish() {
            Ok(()) => Vec::new(),
            Err(error) => vec![Err(error)],
        };
    };
    decoder.push(bytes);
    std::iter::from_fn(|| decoder.next_message()).collect()
}

pub(super) fn watch_output(
    stdout: ChildStdout,
    pid: glib::Pid,
    mut on_bytes: impl FnMut(Option<&[u8]>) + 'static,
    on_exit: impl FnOnce(i32) + 'static,
) {
    let status: Rc<Cell<Option<i32>>> = Rc::default();
    let is_closed = Rc::new(Cell::new(false));
    let on_exit_slot = Rc::new(Cell::new(Some(on_exit)));
    let conclude = {
        let (status, is_closed) = (status.clone(), is_closed.clone());
        move || {
            let Some(status) = status.get().filter(|_| is_closed.get()) else {
                return;
            };
            if let Some(action) = on_exit_slot.take() {
                action(status);
            }
        }
    };
    let last_output_time = Rc::new(Cell::new(Instant::now()));
    readable::add_reader(stdout, {
        let (conclude, is_closed) = (conclude.clone(), is_closed.clone());
        let last_output_time = last_output_time.clone();
        move |bytes| {
            last_output_time.set(Instant::now());
            on_bytes(bytes);
            if bytes.is_none() {
                is_closed.set(true);
                conclude();
            }
        }
    });
    glib::spawn_future_local(async move {
        let (_, code) = glib::child_watch_future(pid).await;
        status.set(Some(code));
        conclude();
        let exit_time = Instant::now();
        glib::timeout_add_local(OUTPUT_CHECK_INTERVAL, move || {
            if is_closed.get() {
                return glib::ControlFlow::Break;
            }
            if last_output_time.get().elapsed() < OUTPUT_GRACE && exit_time.elapsed() < OUTPUT_LIMIT
            {
                return glib::ControlFlow::Continue;
            }
            is_closed.set(true);
            conclude();
            glib::ControlFlow::Break
        });
    });
}

pub(super) fn visible_text(line: &str) -> String {
    let last = line.rsplit('\r').next().unwrap_or(line);
    let mut text = String::with_capacity(last.len());
    let mut characters = last.chars();
    while let Some(character) = characters.next() {
        if character == '\t' || !character.is_control() {
            text.push(character);
            continue;
        }
        if character != '\u{1b}' {
            continue;
        }
        match characters.next() {
            Some('[') => {
                characters.find(|code| ('@'..='~').contains(code));
            }
            Some(']') => {
                if characters.find(|code| matches!(code, '\u{7}' | '\u{1b}')) == Some('\u{1b}') {
                    characters.next();
                }
            }
            Some(' '..='/') => {
                characters.find(|code| !(' '..='/').contains(code));
            }
            _ => {}
        }
    }
    text
}

pub(super) fn exit_code(wait_status: i32) -> Option<i32> {
    (wait_status & 0x7f == 0).then_some((wait_status >> 8) & 0xff)
}

pub(super) fn elapsed_text(microseconds: i64) -> String {
    let seconds = microseconds / 1_000_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proscenio_protocol::Message;
    use std::process::Stdio;

    #[test]
    fn a_message_cut_off_at_the_end_of_the_output_is_a_protocol_error() {
        let frame = Event::Line("upgrading gtk4".to_owned()).encode();
        let mut decoder = Decoder::default();
        assert_eq!(
            decode_events(&mut decoder, Some(&frame)),
            [Ok(Event::Line("upgrading gtk4".to_owned()))]
        );
        assert_eq!(decode_events(&mut decoder, None), []);
        assert_eq!(decode_events(&mut decoder, Some(&frame[..6])), []);
        assert_eq!(decode_events(&mut decoder, None), [Err(Error::Truncated)]);
    }

    fn spawn(line: &[&str]) -> std::process::Child {
        std::process::Command::new(line[0])
            .args(&line[1..])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
    }

    fn count_bytes_read_at_exit(
        context: &glib::MainContext,
        child: &mut std::process::Child,
    ) -> Option<usize> {
        let received_bytes = Rc::new(Cell::new(0));
        let received_bytes_at_exit: Rc<Cell<Option<usize>>> = Rc::default();
        watch_output(
            child.stdout.take().unwrap(),
            glib::Pid(child.id() as i32),
            {
                let received_bytes = received_bytes.clone();
                move |bytes| received_bytes.set(received_bytes.get() + bytes.map_or(0, <[u8]>::len))
            },
            {
                let received_bytes_at_exit = received_bytes_at_exit.clone();
                move |_| received_bytes_at_exit.set(Some(received_bytes.get()))
            },
        );
        let is_timed_out = Rc::new(Cell::new(false));
        let timeout_source = glib::timeout_add_local_once(Duration::from_secs(10), {
            let is_timed_out = is_timed_out.clone();
            move || is_timed_out.set(true)
        });
        while received_bytes_at_exit.get().is_none() && !is_timed_out.get() {
            context.iteration(true);
        }
        if !is_timed_out.get() {
            timeout_source.remove();
        }
        received_bytes_at_exit.get()
    }

    #[test]
    fn a_session_ends_only_after_its_whole_output_is_read() {
        let context = glib::MainContext::default();
        let _owner = context.acquire().unwrap();
        let mut child = spawn(&["head", "-c", "60000", "/dev/zero"]);
        let stat_path = format!("/proc/{}/stat", child.id());
        let exit_deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < exit_deadline
            && !std::fs::read_to_string(&stat_path)
                .unwrap()
                .rsplit_once(") ")
                .is_some_and(|(_, fields)| fields.starts_with('Z'))
        {
            std::thread::yield_now();
        }
        assert_eq!(count_bytes_read_at_exit(&context, &mut child), Some(60000));
        let mut child = spawn(&[
            "sh",
            "-c",
            "(for i in 1 2 3 4 5 6 7 8; do echo line; sleep 0.2; done) & exit 0",
        ]);
        assert_eq!(count_bytes_read_at_exit(&context, &mut child), Some(40));
    }

    #[test]
    fn the_exit_code_comes_from_the_wait_status_unless_a_signal_ended_the_job() {
        assert_eq!(exit_code(0), Some(0));
        assert_eq!(exit_code(1 << 8), Some(1));
        assert_eq!(exit_code(126 << 8), Some(126));
        assert_eq!(exit_code(9), None);
    }

    #[test]
    fn the_elapsed_time_reads_as_minutes_and_seconds() {
        assert_eq!(elapsed_text(0), "0:00");
        assert_eq!(elapsed_text(75_400_000), "1:15");
        assert_eq!(elapsed_text(3_600_000_000), "60:00");
    }

    #[test]
    fn a_progress_meter_line_keeps_only_its_last_redraw() {
        assert_eq!(visible_text("  0  0 \r 50  1M \r100 179M"), "100 179M");
        assert_eq!(visible_text("upgrading a..."), "upgrading a...");
    }

    #[test]
    fn terminal_colors_are_dropped_from_a_line() {
        assert_eq!(
            visible_text("\u{1b}[1m\u{1b}[34m==>\u{1b}[m NOTE:\u{1b}[0;1m flags\u{1b}[m"),
            "==> NOTE: flags"
        );
        assert_eq!(visible_text("old\r\u{1b}[32mnew\u{1b}[0m"), "new");
    }

    #[test]
    fn other_terminal_controls_are_dropped_from_a_line() {
        assert_eq!(
            visible_text("\u{1b}[m\u{f}\u{1b}[1m==>\u{1b}[m\u{f} NOTE:\tflags\u{1b}(B\u{1b}[m"),
            "==> NOTE:\tflags"
        );
        assert_eq!(visible_text("\u{1b}7saved\u{7}\u{1b}8"), "saved");
        assert_eq!(
            visible_text("\u{1b}]0;Building a\u{7}built\u{1b}]2;b\u{1b}\\ a"),
            "built a"
        );
    }
}
