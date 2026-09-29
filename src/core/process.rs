use gtk4::gio;
use gtk4::glib;
use std::cell::Cell;
use std::ffi::OsStr;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::process::{Child, ChildStdout, Command, Output, Stdio};
use std::rc::Rc;

use crate::platform::readable;

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
    let mut command = own_session(&SCOPE);
    command
        .args(unit.map(|unit| format!("--unit={unit}")))
        .arg("--")
        .args(line);
    command
}

pub struct Running {
    pub child: Child,
    exited: Rc<Cell<bool>>,
}

impl Running {
    pub fn stop(&mut self) {
        if !self.exited.get() {
            let _ = self.child.kill();
        }
    }
}

pub fn start(mut command: Command) -> Option<Running> {
    let child = command.spawn().ok()?;
    let exited = Rc::new(Cell::new(false));
    glib::child_watch_add_local(glib::Pid(child.id() as i32), {
        let exited = exited.clone();
        move |_, _| exited.set(true)
    });
    Some(Running { child, exited })
}

pub fn detach(line: &[&str]) {
    start(own_session(line));
}

pub fn launch(line: &[&str]) {
    start(own_scope(None, line));
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

pub fn lines(mut stdout: ChildStdout, mut handler: impl FnMut(Option<&str>) + 'static) {
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let fd = stdout.as_raw_fd();
    readable::when_readable(fd, move || {
        let read = stdout.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            handler(None);
            return glib::ControlFlow::Break;
        }
        pending.extend_from_slice(&chunk[..read]);
        while let Some(end) = pending.iter().position(|&byte| byte == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            handler(Some(String::from_utf8_lossy(&line[..end]).as_ref()));
        }
        glib::ControlFlow::Continue
    });
}

pub fn executable() -> String {
    let path = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "proscenio".to_owned());
    match path.strip_suffix(" (deleted)") {
        Some(replaced) => replaced.to_owned(),
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

pub fn running(name: &str) -> bool {
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
    let command = command(line);
    glib::spawn_future_local(async move {
        if let Some(output) = capture_text(command).await {
            handler(output);
        }
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
