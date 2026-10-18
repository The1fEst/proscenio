use proscenio_protocol::Event;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Stdio};

use crate::output::{emit, finish};
use crate::transaction::{install, parse_native_install};

const PACMAN: &str = "/usr/bin/pacman";

pub fn run(args: &[String]) -> i32 {
    if let Some((files, flags)) = parse_native_install(args) {
        return finish(install(&files, flags));
    }
    emit(&Event::Line(format!("pacman {}", args.join(" "))));
    let child = Command::new(PACMAN)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        return finish(Err(format!("could not run {PACMAN}")));
    };
    let streams: Vec<Box<dyn Read + Send>> = vec![
        Box::new(child.stdout.take().unwrap()),
        Box::new(child.stderr.take().unwrap()),
    ];
    let readers: Vec<_> = streams
        .into_iter()
        .map(|stream| std::thread::spawn(move || relay(stream, |line| emit(&Event::Line(line)))))
        .collect();
    let status = child.wait();
    for reader in readers {
        let _ = reader.join();
    }
    let (exit_code, result) = match status {
        Ok(status) => summarize_exit(status),
        Err(error) => (1, Err(format!("could not wait for {PACMAN}: {error}"))),
    };
    finish(result);
    exit_code
}

fn summarize_exit(status: ExitStatus) -> (i32, Result<(), String>) {
    if let Some(signal) = status.signal() {
        return (
            128 + signal,
            Err(format!("pacman was killed by signal {signal}")),
        );
    }
    let exit_code = status.code().unwrap_or(1);
    let result = if exit_code == 0 {
        Ok(())
    } else {
        Err(format!("pacman exited with {exit_code}"))
    };
    (exit_code, result)
}

fn relay(stream: impl Read, mut on_line: impl FnMut(String)) {
    for line in BufReader::new(stream).split(b'\n').map_while(Result::ok) {
        let line = line.strip_suffix(b"\r").unwrap_or(&line);
        on_line(String::from_utf8_lossy(line).into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pacman_exit_reports_its_code_or_the_signal_that_killed_it() {
        let summarize_script = |script: &str| {
            let status = Command::new("sh").args(["-c", script]).status().unwrap();
            summarize_exit(status)
        };
        assert_eq!(summarize_script("exit 0"), (0, Ok(())));
        assert_eq!(
            summarize_script("exit 3"),
            (3, Err("pacman exited with 3".to_owned()))
        );
        assert_eq!(
            summarize_script("kill -KILL $$"),
            (137, Err("pacman was killed by signal 9".to_owned()))
        );
    }

    #[test]
    fn output_lines_survive_bytes_that_are_not_utf8() {
        let mut lines = Vec::new();
        relay(&b"one\r\n\xe9t\xe9\nthree"[..], |line| lines.push(line));
        assert_eq!(lines, ["one", "\u{fffd}t\u{fffd}", "three"]);
    }
}
