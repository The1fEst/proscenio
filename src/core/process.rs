use gtk4::gio;
use gtk4::glib;
use std::ffi::OsStr;
use std::process::{Command, Stdio};

unsafe extern "C" {
    fn setsid() -> i32;
}

pub fn own_session(flags: gio::SubprocessFlags) -> gio::SubprocessLauncher {
    let launcher = gio::SubprocessLauncher::new(flags);
    launcher.set_child_setup(|| unsafe {
        setsid();
    });
    launcher
}

pub fn detach(command: &[&str]) {
    let launcher =
        own_session(gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE);
    let _ = launcher.spawn(&command.iter().map(OsStr::new).collect::<Vec<_>>());
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

pub fn restart_shell() {
    restart(Command::new(executable()));
}

pub fn restart_shell_on_settings(page: &str) {
    let mut command = Command::new(executable());
    command.env(OPEN_SETTINGS, page);
    restart(command);
}

fn restart(mut command: Command) {
    use std::os::unix::process::CommandExt;
    let _ = command.args(std::env::args_os().skip(1)).exec();
}

pub fn detach_subcommand(arguments: &[&str]) {
    let executable = executable();
    let mut command = vec![executable.as_str()];
    command.extend_from_slice(arguments);
    detach(&command);
}

pub fn exists(program: &str) -> bool {
    glib::find_program_in_path(program).is_some()
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

pub fn read(command: &[&str], handler: impl FnOnce(String) + 'static) {
    let Ok(process) = gio::Subprocess::newv(
        &command.iter().map(OsStr::new).collect::<Vec<_>>(),
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    ) else {
        return;
    };
    glib::spawn_future_local(async move {
        if let Ok((Some(output), _)) = process.communicate_utf8_future(None).await {
            handler(output.to_string());
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
