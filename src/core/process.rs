use gtk4::gio;
use gtk4::glib;
use std::cell::Cell;
use std::ffi::OsStr;
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use std::rc::Rc;

use crate::platform::wayland::readable;

pub fn command<S: AsRef<OsStr>>(line: &[S]) -> Command {
    let Some((program, arguments)) = line.split_first() else {
        return Command::new("true");
    };
    let mut command = Command::new(program);
    command.args(arguments).stdin(Stdio::null());
    command
}

pub fn quiet<S: AsRef<OsStr>>(line: &[S]) -> Command {
    let mut command = command(line);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    command
}

pub fn own_session<S: AsRef<OsStr>>(line: &[S]) -> Command {
    let mut command = quiet(&["setsid"]);
    command.args(line);
    command
}

const SCOPE: [&str; 5] = ["systemd-run", "--user", "--scope", "--quiet", "--collect"];

pub fn own_scope<S: AsRef<OsStr>>(unit: Option<&str>, line: &[S]) -> Command {
    build_scope_command(&["setsid"], unit, line)
}

pub fn build_app_command<S: AsRef<OsStr>>(unit: Option<&str>, line: &[S]) -> Command {
    build_scope_command(&["setsid", "-f"], unit, line)
}

fn build_scope_command<S: AsRef<OsStr>>(
    launcher: &[&str],
    unit: Option<&str>,
    line: &[S],
) -> Command {
    let mut command = quiet(launcher);
    command
        .args(SCOPE)
        .args(unit.map(|unit| format!("--unit={unit}")))
        .arg("--")
        .args(line);
    command
}

pub struct Running {
    pub child: Child,
    has_exited: Rc<Cell<bool>>,
}

impl Running {
    pub fn stop(&mut self) {
        if !self.has_exited.get() {
            let _ = self.child.kill();
        }
    }
}

pub fn start(mut command: Command) -> Option<Running> {
    let child = command.spawn().ok()?;
    let has_exited = Rc::new(Cell::new(false));
    glib::child_watch_add_local(glib::Pid(child.id() as i32), {
        let has_exited = has_exited.clone();
        move |_, _| has_exited.set(true)
    });
    Some(Running { child, has_exited })
}

pub fn detach(line: &[&str]) {
    start(own_session(line));
}

pub fn launch(line: &[&str]) {
    start(build_app_command(None, line));
}

pub async fn finish(mut command: Command) -> Option<bool> {
    let child = command.spawn().ok()?;
    let (_, status) = glib::child_watch_future(glib::Pid(child.id() as i32)).await;
    Some(status == 0)
}

pub async fn capture(mut command: Command) -> Option<Output> {
    gio::spawn_blocking(move || command.output().ok())
        .await
        .ok()
        .flatten()
}

pub async fn capture_text(mut command: Command) -> Option<String> {
    command.stderr(Stdio::null());
    let output = capture(command).await?;
    String::from_utf8(output.stdout).ok()
}

pub fn lines(stdout: ChildStdout, handler: impl FnMut(Option<&str>) + 'static) {
    readable::add_reader(stdout, split_lines(handler));
}

pub fn split_lines(mut handler: impl FnMut(Option<&str>)) -> impl FnMut(Option<&[u8]>) {
    let mut buffer: Vec<u8> = Vec::new();
    move |bytes| {
        let Some(bytes) = bytes else {
            if !buffer.is_empty() {
                handler(Some(String::from_utf8_lossy(&buffer).as_ref()));
                buffer.clear();
            }
            handler(None);
            return;
        };
        buffer.extend_from_slice(bytes);
        while let Some(end) = buffer.iter().position(|&byte| byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=end).collect();
            handler(Some(String::from_utf8_lossy(&line[..end]).as_ref()));
        }
    }
}

pub fn executable() -> String {
    let path = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "proscenio".to_owned());
    match path.strip_suffix(" (deleted)") {
        Some(original_path) => original_path.to_owned(),
        None => path,
    }
}

pub const OPEN_SETTINGS: &str = "PROSCENIO_OPEN_SETTINGS";
pub const OPEN_WELCOME: &str = "PROSCENIO_OPEN_WELCOME";

pub fn restart_shell() {
    restart(Command::new(executable()));
}

pub fn restart_shell_on_settings(page: &str) {
    let mut command = Command::new(executable());
    command.env(OPEN_SETTINGS, page);
    restart(command);
}

pub fn restart_shell_on_welcome() {
    let mut command = Command::new(executable());
    command.env(OPEN_WELCOME, "1");
    restart(command);
}

fn restart(mut command: Command) {
    use std::os::unix::process::CommandExt;
    let _ = command.args(std::env::args_os().skip(1)).exec();
}

pub fn launch_subcommand(arguments: &[&str]) {
    let executable = executable();
    let mut command = vec![executable.as_str()];
    command.extend_from_slice(arguments);
    launch(&command);
}

pub fn exists(program: &str) -> bool {
    glib::find_program_in_path(program).is_some()
}

pub fn is_running(name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with(|c: char| c.is_ascii_digit())
            && std::fs::read_to_string(entry.path().join("comm"))
                .is_ok_and(|comm| comm.trim_end() == name)
    })
}

pub fn ancestors(pid: u32) -> Vec<u32> {
    let mut chain = vec![pid];
    while let Some(parent) = chain
        .last()
        .and_then(|pid| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok())
        .and_then(|stat| parent_in(&stat))
        .filter(|parent| *parent > 1)
    {
        chain.push(parent);
    }
    chain
}

fn parent_in(stat: &str) -> Option<u32> {
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

pub fn run<S: AsRef<OsStr>>(command: &[S]) -> bool {
    let Some((program, arguments)) = command.split_first() else {
        return false;
    };
    Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn read(line: &[&str], handler: impl FnOnce(String) + 'static) {
    read_optional(line, |output| {
        if let Some(output) = output {
            handler(output);
        }
    });
}

pub fn read_optional(line: &[&str], handler: impl FnOnce(Option<String>) + 'static) {
    let command = command(line);
    glib::spawn_future_local(async move {
        handler(capture_text(command).await);
    });
}

pub fn output<S: AsRef<OsStr>>(command: &[S]) -> Option<String> {
    let (program, arguments) = command.split_first()?;
    let output = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.trim_end_matches('\n').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_forks_away_from_the_shell_into_its_own_scope() {
        let command = build_app_command(Some("app-x.scope"), &["firefox", "--new-window"]);
        assert_eq!(command.get_program(), "setsid");
        let arguments: Vec<_> = command.get_args().collect();
        assert_eq!(
            arguments,
            [
                "-f",
                "systemd-run",
                "--user",
                "--scope",
                "--quiet",
                "--collect",
                "--unit=app-x.scope",
                "--",
                "firefox",
                "--new-window"
            ]
        );
    }

    #[test]
    fn the_parent_follows_a_command_name_with_spaces_and_parentheses() {
        assert_eq!(
            parent_in("4242 (Web Content (x)) S 4100 4242 4100 0 -1 4194560"),
            Some(4100)
        );
        assert_eq!(parent_in("garbage"), None);
    }

    #[test]
    fn a_last_line_without_a_newline_arrives_before_the_end() {
        let split = |chunks: &[&[u8]]| {
            let mut lines = Vec::new();
            let mut on_bytes = split_lines(|line| lines.push(line.map(str::to_owned)));
            for chunk in chunks {
                on_bytes(Some(chunk));
            }
            on_bytes(None);
            drop(on_bytes);
            lines
        };
        assert_eq!(
            split(&[b"first\nsec", b"ond\nlast"]),
            [
                Some("first".to_owned()),
                Some("second".to_owned()),
                Some("last".to_owned()),
                None
            ]
        );
        assert_eq!(split(&[b"done\n"]), [Some("done".to_owned()), None]);
    }

    #[test]
    fn this_process_leads_up_to_its_parent() {
        let chain = ancestors(std::process::id());
        assert_eq!(chain[0], std::process::id());
        assert_eq!(
            chain.get(1).copied(),
            Some(std::os::unix::process::parent_id())
        );
    }
}
