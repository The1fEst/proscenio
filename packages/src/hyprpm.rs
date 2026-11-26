use proscenio_protocol::{ERROR_PREFIX, Event, Request};
use std::ffi::{CStr, c_char};
use std::fs;
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use crate::errors::format_error;
use crate::export::{Entry, decode_entries};
use crate::files::{create_dirs, ensure_real_dirs, is_name, remove_dir, write_file};
use crate::headers::replace_headers;
use crate::output::emit;
use crate::pacman::relay;
use crate::session::{TrustScope, Work, open_session, read_session_uid};

const DATA_ROOT: &str = "/var/cache/hyprpm";
const HEADERS_DIR: &str = "headersRoot";
const STAGE_DIR: &str = "proscenio-headers";
const MAKE: &str = "/usr/bin/make";
const EXPORTER: &str = "/usr/lib/proscenio/proscenio-packages";
const OUTPUT_GRACE: Duration = Duration::from_secs(1);
const MAX_EXPORT_BYTES: u64 = 64 << 20;
const MAX_ERROR_BYTES: u64 = 64 << 10;
const MAX_OUTPUT_BYTES: u64 = 4 << 20;
const STATE_FILE: &str = "state.toml";

#[repr(C)]
struct Passwd {
    name: *const c_char,
    password: *const c_char,
    uid: u32,
    gid: u32,
    gecos: *const c_char,
    home: *const c_char,
    shell: *const c_char,
}

unsafe extern "C" {
    fn getpwuid(uid: u32) -> *const Passwd;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub uid: u32,
    pub gid: u32,
    pub name: String,
}

impl Account {
    fn data_dir(&self) -> String {
        format!("{DATA_ROOT}/{}", self.name)
    }

    fn headers_dir(&self) -> String {
        format!("{}/{HEADERS_DIR}", self.data_dir())
    }

    fn temp_root(&self) -> String {
        format!("/run/user/{}/hyprpm", self.uid)
    }

    fn build_dir(&self) -> String {
        format!("{}/hyprland-{}", self.temp_root(), self.name)
    }

    fn headers_command(&self) -> String {
        let (build_dir, headers_dir) = (self.build_dir(), self.headers_dir());
        format!(
            "make -C '{build_dir}' installheaders && chmod -R 644 '{headers_dir}' && find '{headers_dir}' -type d -exec chmod a+x {{}} \\;"
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    CheckAccess,
    CreateDir(PathBuf),
    RemoveRepository(PathBuf),
    Install {
        source: PathBuf,
        target: PathBuf,
        mode: u32,
    },
    InstallHeaders,
}

fn lookup_account(uid: u32) -> Result<Account, String> {
    let entry = unsafe { getpwuid(uid) };
    if entry.is_null() {
        return Err(format!("no account has uid {uid}"));
    }
    let (name, gid) = unsafe {
        (
            CStr::from_ptr((*entry).name).to_string_lossy().into_owned(),
            (*entry).gid,
        )
    };
    if !is_plain_name(&name) {
        return Err(format!("the account name {name:?} is not a plain name"));
    }
    Ok(Account { uid, gid, name })
}

fn is_plain_name(part: &str) -> bool {
    !part.starts_with('.')
        && is_name(part)
        && part
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

fn is_repository_name(part: &str) -> bool {
    is_plain_name(part) && part != HEADERS_DIR && part != STATE_FILE
}

fn is_inside(path: &str, base: &str) -> bool {
    let Some(rest) = path
        .strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return false;
    };
    let parts: Vec<&str> = rest
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    !parts.is_empty() && parts.iter().all(|part| is_name(part))
}

fn split_under<'a>(path: &'a str, base: &str) -> Option<Vec<&'a str>> {
    let rest = path.strip_prefix(base)?.strip_prefix('/')?;
    let parts: Vec<&str> = rest.split('/').collect();
    parts.iter().all(|part| is_name(part)).then_some(parts)
}

fn parse_step(args: &[String], account: &Account) -> Result<Step, String> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let data_dir = account.data_dir();
    let refusal = || format!("not a step hyprpm update runs as root: {words:?}");
    match words.as_slice() {
        ["echo", "hyprland"] => Ok(Step::CheckAccess),
        ["mkdir", "-p", "-m", "755", path] => {
            let is_allowed = *path == DATA_ROOT
                || *path == data_dir
                || *path == account.headers_dir()
                || split_under(path, &data_dir)
                    .is_some_and(|parts| matches!(parts[..], [part] if is_repository_name(part)));
            is_allowed
                .then(|| Step::CreateDir(PathBuf::from(path)))
                .ok_or_else(refusal)
        }
        ["rm", "-fr", path] => split_under(path, &data_dir)
            .is_some_and(|parts| matches!(parts[..], [part] if is_repository_name(part)))
            .then(|| Step::RemoveRepository(PathBuf::from(path)))
            .ok_or_else(refusal),
        ["install", mode, "-o", "0", "-g", "0", source, target] => {
            parse_install(mode, source, target, account).ok_or_else(refusal)
        }
        ["/bin/sh", "-c", command] if *command == account.headers_command() => {
            Ok(Step::InstallHeaders)
        }
        _ => Err(refusal()),
    }
}

fn parse_install(mode: &str, source: &str, target: &str, account: &Account) -> Option<Step> {
    let parts = split_under(target, &account.data_dir())?;
    let temp_root = account.temp_root();
    let is_state = match parts[..] {
        [file] => file == STATE_FILE,
        [repository, file] => is_repository_name(repository) && file == STATE_FILE,
        _ => false,
    };
    let is_plugin = matches!(
        parts[..],
        [repository, file] if is_repository_name(repository)
            && is_plain_name(file)
            && file.ends_with(".so")
    );
    let mode = if is_state && mode == "-m644" && source == format!("{temp_root}/.temp-state") {
        0o644
    } else if is_plugin
        && mode == "-m0755"
        && is_inside(source, &format!("{temp_root}/{}", account.name))
    {
        0o755
    } else {
        return None;
    };
    Some(Step::Install {
        source: PathBuf::from(source),
        target: PathBuf::from(target),
        mode,
    })
}

pub fn run_hyprpm_session() -> Result<(), String> {
    let result = serve_hyprpm_steps();
    if let Err(message) = &result {
        emit(&Event::Line(format!("{ERROR_PREFIX}{message}")));
    }
    result
}

fn serve_hyprpm_steps() -> Result<(), String> {
    let uid = read_session_uid()?;
    let account = lookup_account(uid)?;
    let session = open_session(uid, TrustScope::Children)?;
    for work in &session.receiver {
        match work {
            Work::Request(Request::Quit) | Work::Closed => break,
            Work::Request(_) => {}
            Work::Forward(forward, stream) => {
                session.answer(forward, stream, |args| run_step(args, &account));
            }
        }
    }
    Ok(())
}

fn run_step(args: &[String], account: &Account) -> i32 {
    let result = parse_step(args, account).and_then(|step| execute(&step, account));
    match result {
        Ok(()) => 0,
        Err(message) => {
            emit(&Event::Line(format!("{ERROR_PREFIX}{message}")));
            1
        }
    }
}

fn execute(step: &Step, account: &Account) -> Result<(), String> {
    match step {
        Step::CheckAccess => Ok(()),
        Step::CreateDir(path) => create_dirs(path),
        Step::RemoveRepository(path) => remove_repository(path),
        Step::Install {
            source,
            target,
            mode,
        } => match export_as_user(account, source)?.as_slice() {
            [Entry::File(path, bytes)] if path.is_empty() => {
                ensure_real_dirs(target.parent().unwrap_or(Path::new("/")))?;
                write_file(target, bytes, *mode)
            }
            _ => Err(format!("{} is not a regular file", source.display())),
        },
        Step::InstallHeaders => install_headers(account),
    }
}

fn remove_repository(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_dir()) {
        return Err(format!("{} is not a directory", path.display()));
    }
    remove_dir(path)
}

fn build_user_command(account: &Account, program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .uid(account.uid)
        .gid(account.gid)
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("LANG", "C")
        .stdin(Stdio::null());
    command
}

fn read_capped(source: impl Read, limit: u64) -> io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    source.take(limit + 1).read_to_end(&mut bytes)?;
    let is_capped = bytes.len() as u64 > limit;
    bytes.truncate(limit as usize);
    Ok((bytes, is_capped))
}

fn export_as_user(account: &Account, path: &Path) -> Result<Vec<Entry>, String> {
    let mut child = build_user_command(account, EXPORTER)
        .arg("export")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(format_error)?;
    let errors = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let errors = read_capped(&mut stderr, MAX_ERROR_BYTES).map(|(bytes, _)| bytes);
            let _ = io::copy(&mut stderr, &mut io::sink());
            errors
        })
    });
    let output = child
        .stdout
        .take()
        .map(|stdout| read_capped(stdout, MAX_EXPORT_BYTES))
        .transpose();
    let is_capped = output
        .as_ref()
        .is_ok_and(|output| output.as_ref().is_some_and(|(_, is_capped)| *is_capped));
    if output.is_err() || is_capped {
        let _ = child.kill();
    }
    let status = child.wait();
    let errors = errors
        .and_then(|reader| reader.join().ok())
        .and_then(Result::ok)
        .unwrap_or_default();
    let (bytes, _) = output.map_err(format_error)?.unwrap_or_default();
    if is_capped {
        return Err(format!(
            "{} is larger than {} MiB",
            path.display(),
            MAX_EXPORT_BYTES >> 20
        ));
    }
    if !status.map_err(format_error)?.success() {
        let errors = String::from_utf8_lossy(&errors);
        let errors = errors.trim().trim_start_matches(ERROR_PREFIX).to_owned();
        return Err(if errors.is_empty() {
            format!("could not read {}", path.display())
        } else {
            errors
        });
    }
    decode_entries(&bytes)
}

fn install_headers(account: &Account) -> Result<(), String> {
    let stage = format!("{}/{STAGE_DIR}", account.build_dir());
    let (reader, writer) = io::pipe().map_err(format_error)?;
    let mut child = {
        let mut command = build_user_command(account, MAKE);
        command
            .arg("-C")
            .arg(account.build_dir())
            .arg("installheaders")
            .arg(format!("PREFIX={stage}"))
            .stdout(writer.try_clone().map_err(format_error)?)
            .stderr(writer);
        command.spawn().map_err(format_error)?
    };
    let (completion_sender, completion) = mpsc::channel();
    let is_step_over = Arc::new(AtomicBool::new(false));
    std::thread::spawn({
        let is_step_over = is_step_over.clone();
        move || {
            let mut output = reader.take(MAX_OUTPUT_BYTES);
            relay(&mut output, |line| {
                if !is_step_over.load(Ordering::SeqCst) {
                    emit(&Event::Line(line));
                }
            });
            let _ = io::copy(&mut output.into_inner(), &mut io::sink());
            let _ = completion_sender.send(());
        }
    });
    let status = child.wait().map_err(format_error)?;
    let _ = completion.recv_timeout(OUTPUT_GRACE);
    is_step_over.store(true, Ordering::SeqCst);
    if !status.success() {
        return Err("make installheaders failed".to_owned());
    }
    let entries = export_as_user(account, Path::new(&stage))?;
    replace_headers(Path::new(&account.headers_dir()), &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_account() -> Account {
        Account {
            uid: 1000,
            gid: 1000,
            name: "fest".to_owned(),
        }
    }

    fn to_words(line: &[&str]) -> Vec<String> {
        line.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn only_the_root_steps_of_hyprpm_update_are_allowed() {
        let headers_command = build_account().headers_command();
        let allowed = [
            (vec!["echo", "hyprland"], Step::CheckAccess),
            (
                vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm"],
                Step::CreateDir("/var/cache/hyprpm".into()),
            ),
            (
                vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/fest"],
                Step::CreateDir("/var/cache/hyprpm/fest".into()),
            ),
            (
                vec![
                    "mkdir",
                    "-p",
                    "-m",
                    "755",
                    "/var/cache/hyprpm/fest/headersRoot",
                ],
                Step::CreateDir("/var/cache/hyprpm/fest/headersRoot".into()),
            ),
            (
                vec![
                    "mkdir",
                    "-p",
                    "-m",
                    "755",
                    "/var/cache/hyprpm/fest/proscenio",
                ],
                Step::CreateDir("/var/cache/hyprpm/fest/proscenio".into()),
            ),
            (
                vec!["rm", "-fr", "/var/cache/hyprpm/fest/proscenio"],
                Step::RemoveRepository("/var/cache/hyprpm/fest/proscenio".into()),
            ),
            (
                vec![
                    "install",
                    "-m644",
                    "-o",
                    "0",
                    "-g",
                    "0",
                    "/run/user/1000/hyprpm/.temp-state",
                    "/var/cache/hyprpm/fest/state.toml",
                ],
                Step::Install {
                    source: "/run/user/1000/hyprpm/.temp-state".into(),
                    target: "/var/cache/hyprpm/fest/state.toml".into(),
                    mode: 0o644,
                },
            ),
            (
                vec![
                    "install",
                    "-m644",
                    "-o",
                    "0",
                    "-g",
                    "0",
                    "/run/user/1000/hyprpm/.temp-state",
                    "/var/cache/hyprpm/fest/proscenio/state.toml",
                ],
                Step::Install {
                    source: "/run/user/1000/hyprpm/.temp-state".into(),
                    target: "/var/cache/hyprpm/fest/proscenio/state.toml".into(),
                    mode: 0o644,
                },
            ),
            (
                vec![
                    "install",
                    "-m0755",
                    "-o",
                    "0",
                    "-g",
                    "0",
                    "/run/user/1000/hyprpm/fest/plugin/proscenio_hyprland.so",
                    "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
                ],
                Step::Install {
                    source: "/run/user/1000/hyprpm/fest/plugin/proscenio_hyprland.so".into(),
                    target: "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so".into(),
                    mode: 0o755,
                },
            ),
            (
                vec![
                    "install",
                    "-m0755",
                    "-o",
                    "0",
                    "-g",
                    "0",
                    "/run/user/1000/hyprpm/fest/./build//x.so",
                    "/var/cache/hyprpm/fest/other/x.so",
                ],
                Step::Install {
                    source: "/run/user/1000/hyprpm/fest/./build//x.so".into(),
                    target: "/var/cache/hyprpm/fest/other/x.so".into(),
                    mode: 0o755,
                },
            ),
            (
                vec!["/bin/sh", "-c", &headers_command],
                Step::InstallHeaders,
            ),
        ];
        for (line, expected) in allowed {
            assert_eq!(
                parse_step(&to_words(&line), &build_account()),
                Ok(expected),
                "{line:?}"
            );
        }
    }

    #[test]
    fn every_other_root_step_is_refused() {
        let headers_command = build_account().headers_command();
        let chained = format!("{headers_command}; rm -rf /");
        let other_build = headers_command.replace("hyprland-fest", "hyprland-root");
        let state = "/run/user/1000/hyprpm/.temp-state";
        let plugin = "/run/user/1000/hyprpm/fest/plugin/proscenio_hyprland.so";
        let refused: Vec<Vec<&str>> = vec![
            vec![],
            vec!["echo"],
            vec!["echo", "hyprland", "extra"],
            vec!["/bin/sh", "-c", "id"],
            vec!["sh", "-c", &headers_command],
            vec!["/bin/sh", "-c", &chained],
            vec!["/bin/sh", "-c", &other_build],
            vec!["/bin/sh", "-c", &headers_command, "extra"],
            vec![
                "mkdir",
                "-p",
                "-m",
                "777",
                "/var/cache/hyprpm/fest/proscenio",
            ],
            vec!["mkdir", "-p", "-m", "755", "/etc/cron.d"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/root"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/fest/../root"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/fest/a/b"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/fest/"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm//fest"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/fest/.hidden"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/fest/a b"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/fest/a\nb"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/fest/a\0b"],
            vec!["mkdir", "-p", "-m", "755", "/var/cache/hyprpm/festival"],
            vec!["mkdir", "-m", "755", "-p", "/var/cache/hyprpm/fest"],
            vec!["rm", "-fr", "/"],
            vec!["rm", "-fr", "/var/cache/hyprpm"],
            vec!["rm", "-fr", "/var/cache/hyprpm/fest"],
            vec!["rm", "-fr", "/var/cache/hyprpm/fest/headersRoot"],
            vec!["rm", "-fr", "/var/cache/hyprpm/fest/state.toml"],
            vec![
                "mkdir",
                "-p",
                "-m",
                "755",
                "/var/cache/hyprpm/fest/state.toml",
            ],
            vec![
                "mkdir",
                "-p",
                "-m",
                "755",
                "/var/cache/hyprpm/fest/headersRoot/x",
            ],
            vec!["rm", "-fr", "/var/cache/hyprpm/fest/.."],
            vec!["rm", "-fr", "/var/cache/hyprpm/fest/proscenio/.."],
            vec!["rm", "-fr", "/var/cache/hyprpm/root/proscenio"],
            vec!["rm", "-rf", "/var/cache/hyprpm/fest/proscenio"],
            vec!["rm", "-fr", "/var/cache/hyprpm/fest/proscenio", "/etc"],
            vec![
                "install",
                "-m644",
                "-o",
                "0",
                "-g",
                "0",
                state,
                "/etc/passwd",
            ],
            vec![
                "install",
                "-m644",
                "-o",
                "0",
                "-g",
                "0",
                "/etc/shadow",
                "/var/cache/hyprpm/fest/state.toml",
            ],
            vec![
                "install",
                "-m4755",
                "-o",
                "0",
                "-g",
                "0",
                plugin,
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
            ],
            vec![
                "install",
                "-m644",
                "-o",
                "0",
                "-g",
                "0",
                plugin,
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                state,
                "/var/cache/hyprpm/fest/state.toml",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "1000",
                "-g",
                "0",
                plugin,
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                "/run/user/1000/hyprpm/fest/../../../../etc/shadow",
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                "/run/user/1000/hyprpm/fest/./build/../../x.so",
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                "/run/user/1000/hyprpm/fest/.//",
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                "/run/user/1000/hyprpm/festival/x.so",
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                "/run/user/1001/hyprpm/fest/plugin/proscenio_hyprland.so",
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                plugin,
                "/var/cache/hyprpm/fest/proscenio/../../../../usr/lib/libc.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                plugin,
                "/var/cache/hyprpm/fest/headersRoot/x.so",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                plugin,
                "/var/cache/hyprpm/fest/proscenio/proscenio_hyprland.sh",
            ],
            vec![
                "install",
                "-m0755",
                "-o",
                "0",
                "-g",
                "0",
                plugin,
                "/var/cache/hyprpm/fest/proscenio/sub/proscenio_hyprland.so",
            ],
            vec!["pacman", "-S", "anything"],
            vec!["bash"],
        ];
        for line in refused {
            assert!(
                parse_step(&to_words(&line), &build_account()).is_err(),
                "{line:?}"
            );
        }
    }

    #[test]
    fn output_past_the_cap_is_dropped() {
        assert_eq!(
            read_capped(&b"abcdef"[..], 4).unwrap(),
            (b"abcd".to_vec(), true)
        );
        assert_eq!(
            read_capped(&b"abcd"[..], 4).unwrap(),
            (b"abcd".to_vec(), false)
        );
        assert_eq!(read_capped(&b""[..], 4).unwrap(), (Vec::new(), false));
    }

    #[test]
    fn the_headers_command_is_the_one_hyprpm_formats() {
        assert_eq!(
            build_account().headers_command(),
            "make -C '/run/user/1000/hyprpm/hyprland-fest' installheaders && chmod -R 644 '/var/cache/hyprpm/fest/headersRoot' && find '/var/cache/hyprpm/fest/headersRoot' -type d -exec chmod a+x {} \\;"
        );
    }

    #[test]
    fn only_plain_names_count_as_repository_and_account_names() {
        for name in ["proscenio", "hypr-plugins", "a_b.c", "fest2"] {
            assert!(is_plain_name(name), "{name}");
        }
        for name in [
            "", ".", "..", ".hidden", "a/b", "a b", "a\nb", "a\0b", "ü", "$x",
        ] {
            assert!(!is_plain_name(name), "{name:?}");
        }
    }
}
