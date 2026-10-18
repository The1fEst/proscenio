use alpm::{
    Alpm, AnyDownloadEvent, AnyEvent, AnyQuestion, DownloadEvent, Event as AlpmEvent, HookWhen,
    LogLevel, Progress, Question, TransFlag,
};
use proscenio_protocol::{
    Change, Event, Exit, Forward, Message, Request, SOCKET_VARIABLE, Stage, TOKEN_LENGTH,
    TOKEN_VARIABLE, VERSION, hex, read_one, unhex,
};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::ffi::{c_int, c_void};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const PACMAN: &str = "/usr/bin/pacman";
const FORWARD_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
const MAX_PENDING_FORWARDS: usize = 8;
const MAX_PROCESS_DEPTH: usize = 256;
const DOWNLOAD_STEPS: u64 = 200;
const SOL_SOCKET: c_int = 1;
const SO_PEERCRED: c_int = 17;
const CHECK_LOCK_TIMEOUT: Duration = Duration::from_secs(120);
const CHECK_LOCK_RETRY_DELAY: Duration = Duration::from_millis(250);
const CHECK_LOCK: &str = "proscenio-check.lock";
const ALPM_LOCK: &str = "db.lck";
const STAGE_DIR: &str = "proscenio-stage";
const SYNC_DIR: &str = "sync";
const LOCAL_DIR: &str = "local";
const MAX_LISTED_CHANGES: usize = 5;
const SYSTEM_HOOK_DIR: &str = "/usr/share/libalpm/hooks/";

#[repr(C)]
#[derive(Default)]
struct Credentials {
    pid: i32,
    uid: u32,
    gid: u32,
}

unsafe extern "C" {
    fn getsockopt(
        socket: c_int,
        level: c_int,
        name: c_int,
        value: *mut c_void,
        length: *mut u32,
    ) -> c_int;
}

enum Work {
    Request(Request),
    Forward(Forward, UnixStream),
    Closed,
}

#[derive(Default)]
struct Downloads {
    files: HashMap<String, (u64, u64)>,
    expected_bytes: u64,
    reported_bytes: u64,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exit_code = match args.first().map(String::as_str) {
        Some("check") if args.len() == 2 => report(check(&args[1])),
        Some("session") if args.len() == 1 => report(run_session()),
        Some(program) if is_pacman(program) => forward(&args[1..]),
        _ => {
            eprintln!("usage: proscenio-packages check DBPATH | session | pacman ARGUMENT…");
            2
        }
    };
    std::process::exit(exit_code);
}

fn is_pacman(program: &str) -> bool {
    program == "pacman" || program.ends_with("/pacman")
}

fn report(result: Result<(), String>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("proscenio-packages: {message}");
            1
        }
    }
}

fn format_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn describe_path_error(path: &Path, error: std::io::Error) -> String {
    format!("{}: {error}", path.display())
}

fn open_alpm(db_path: Option<&str>) -> Result<Alpm, String> {
    let pacman_config = pacmanconf::Config::new().map_err(format_error)?;
    let db_path = db_path.unwrap_or(&pacman_config.db_path);
    let mut alpm = Alpm::new(&*pacman_config.root_dir, db_path).map_err(format_error)?;
    alpm_utils::configure_alpm(&mut alpm, &pacman_config).map_err(format_error)?;
    alpm.set_hookdirs(list_hook_dirs(&pacman_config.hook_dir).iter())
        .map_err(format_error)?;
    Ok(alpm)
}

fn list_hook_dirs(configured_dirs: &[String]) -> Vec<String> {
    let mut hook_dirs = vec![SYSTEM_HOOK_DIR.to_owned()];
    hook_dirs.extend(
        configured_dirs
            .iter()
            .filter(|dir| dir.as_str() != SYSTEM_HOOK_DIR)
            .cloned(),
    );
    hook_dirs
}

fn lock_check_database(db_path: &Path) -> Result<std::fs::File, String> {
    let lock_file = std::fs::File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(db_path.join(CHECK_LOCK))
        .map_err(format_error)?;
    let deadline = std::time::Instant::now() + CHECK_LOCK_TIMEOUT;
    loop {
        match lock_file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(CHECK_LOCK_RETRY_DELAY);
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err("another check is still running".to_owned());
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(format_error(error)),
        }
    }
    match std::fs::remove_file(db_path.join(ALPM_LOCK)) {
        Ok(()) => Ok(lock_file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(lock_file),
        Err(error) => Err(format_error(error)),
    }
}

fn check(db_path: &str) -> Result<(), String> {
    let _lock_file = lock_check_database(Path::new(db_path))?;
    let mut alpm = open_alpm(Some(db_path))?;
    alpm.set_sandbox_user(None::<&str>).map_err(format_error)?;
    alpm.set_disable_sandbox_filesystem(true);
    alpm.set_disable_sandbox_syscalls(true);
    alpm.syncdbs_mut().update(false).map_err(format_error)?;
    let ignored_packages: Vec<String> = alpm.ignorepkgs().iter().map(str::to_owned).collect();
    let mut stdout = std::io::stdout().lock();
    for package in alpm.localdb().pkgs() {
        let Some(sync_package) = package.sync_new_version(alpm.syncdbs()) else {
            continue;
        };
        if ignored_packages.iter().any(|name| name == package.name()) {
            continue;
        }
        writeln!(
            stdout,
            "{} {} -> {}",
            package.name(),
            package.version().as_str(),
            sync_package.version().as_str()
        )
        .map_err(format_error)?;
    }
    Ok(())
}

fn describe_change(change: &Change) -> String {
    let from = if change.from.is_empty() {
        "-"
    } else {
        &change.from
    };
    let to = if change.to.is_empty() {
        "removed"
    } else {
        &change.to
    };
    format!("{} {from} -> {to}", change.name)
}

fn list_changes<'a>(mut descriptions: impl Iterator<Item = &'a String>) -> String {
    let mut listed: Vec<String> = descriptions
        .by_ref()
        .take(MAX_LISTED_CHANGES)
        .cloned()
        .collect();
    let unlisted_count = descriptions.count();
    if unlisted_count > 0 {
        listed.push(format!("and {unlisted_count} more"));
    }
    listed.join(", ")
}

fn compare_changes(confirmed: &[Change], actual: &[Change]) -> Result<(), String> {
    let describe_all =
        |changes: &[Change]| -> BTreeSet<String> { changes.iter().map(describe_change).collect() };
    let (confirmed, actual) = (describe_all(confirmed), describe_all(actual));
    if confirmed == actual {
        return Ok(());
    }
    Err(format!(
        "the packages to update changed since they were listed, nothing was installed (now: {}; no longer: {})",
        list_changes(actual.difference(&confirmed)),
        list_changes(confirmed.difference(&actual)),
    ))
}

fn prepare_stage(db_path: &Path) -> Result<PathBuf, String> {
    let stage = db_path.join(STAGE_DIR);
    match std::fs::remove_dir_all(&stage) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format_error(error)),
    }
    let staged_sync = stage.join(SYNC_DIR);
    std::fs::create_dir(&stage).map_err(|error| describe_path_error(&stage, error))?;
    std::fs::create_dir(&staged_sync).map_err(|error| describe_path_error(&staged_sync, error))?;
    std::os::unix::fs::symlink(db_path.join(LOCAL_DIR), stage.join(LOCAL_DIR))
        .map_err(|error| describe_path_error(&stage, error))?;
    seed_stage_databases(&db_path.join(SYNC_DIR), &staged_sync)?;
    Ok(stage)
}

fn list_databases(sync: &Path) -> Result<Vec<PathBuf>, String> {
    let mut databases = Vec::new();
    for entry in std::fs::read_dir(sync).map_err(|error| describe_path_error(sync, error))? {
        let path = entry
            .map_err(|error| describe_path_error(sync, error))?
            .path();
        let is_database = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".db") || name.ends_with(".db.sig"));
        if is_database {
            databases.push(path);
        }
    }
    Ok(databases)
}

fn seed_stage_databases(system_sync: &Path, staged_sync: &Path) -> Result<(), String> {
    for path in list_databases(system_sync)? {
        let Some(name) = path.file_name() else {
            continue;
        };
        let target = staged_sync.join(name);
        let modified = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .map_err(|error| describe_path_error(&path, error))?;
        std::fs::copy(&path, &target).map_err(|error| describe_path_error(&path, error))?;
        std::fs::File::options()
            .write(true)
            .open(&target)
            .and_then(|file| file.set_modified(modified))
            .map_err(|error| describe_path_error(&target, error))?;
    }
    Ok(())
}

fn install_staged_databases(staged_sync: &Path, system_sync: &Path) -> Result<(), String> {
    for path in list_databases(staged_sync)? {
        let Some(name) = path.file_name() else {
            continue;
        };
        std::fs::rename(&path, system_sync.join(name))
            .map_err(|error| describe_path_error(&path, error))?;
    }
    Ok(())
}

fn find_system_database_path() -> Result<PathBuf, String> {
    let pacman_config = pacmanconf::Config::new().map_err(format_error)?;
    Ok(PathBuf::from(pacman_config.db_path))
}

fn path_to_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("{} is not valid text", path.display()))
}

fn discard_stage() {
    if let Ok(db_path) = find_system_database_path() {
        let _ = std::fs::remove_dir_all(db_path.join(STAGE_DIR));
    }
}

fn collect_changes(alpm: &Alpm) -> Vec<Change> {
    let installed_version = |name: &str| {
        alpm.localdb()
            .pkg(name)
            .map(|package| package.version().as_str().to_owned())
            .unwrap_or_default()
    };
    let mut changes: Vec<Change> = alpm
        .trans_add()
        .iter()
        .map(|package| {
            Change::new(
                package.name(),
                &installed_version(package.name()),
                package.version().as_str(),
            )
        })
        .collect();
    changes.extend(
        alpm.trans_remove()
            .iter()
            .map(|package| Change::new(package.name(), package.version().as_str(), "")),
    );
    changes.sort_by(|left, right| left.name.cmp(&right.name));
    changes
}

fn plan_changes(alpm: &mut Alpm) -> Result<Vec<Change>, String> {
    alpm.trans_init(TransFlag::NONE).map_err(format_error)?;
    let result = alpm
        .sync_sysupgrade(false)
        .map_err(format_error)
        .and_then(|()| {
            alpm.trans_prepare()
                .map_err(|error| format!("{error}: {:?}", error.data()))
        })
        .map(|()| collect_changes(alpm));
    let _ = alpm.trans_release();
    result
}

fn plan() -> Result<(), String> {
    let stage = prepare_stage(&find_system_database_path()?)?;
    let mut alpm = open_alpm(Some(path_to_str(&stage)?))?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Synchronize));
    alpm.syncdbs_mut().update(false).map_err(format_error)?;
    emit(&Event::Stage(Stage::Prepare));
    emit(&Event::Planned(plan_changes(&mut alpm)?));
    Ok(())
}

fn emit(event: &Event) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(&event.encode());
    let _ = stdout.flush();
}

fn run_session() -> Result<(), String> {
    let session_uid = std::env::var("PKEXEC_UID")
        .ok()
        .and_then(|uid| uid.parse::<u32>().ok());
    let Some(session_uid) = session_uid.filter(|_| is_effective_root()) else {
        return Err("the session runs as root, through pkexec".to_owned());
    };
    let token = read_random_bytes(TOKEN_LENGTH)?;
    let name = format!("proscenio-packages-{}", hex(&read_random_bytes(8)?));
    let address = SocketAddr::from_abstract_name(name.as_bytes()).map_err(format_error)?;
    let listener = UnixListener::bind_addr(&address).map_err(format_error)?;
    emit(&Event::Ready {
        version: VERSION,
        socket: name,
        token: token.clone(),
    });

    let (sender, receiver) = mpsc::channel::<Work>();
    let allowed_pids: Arc<Mutex<Vec<u32>>> = Arc::default();
    std::thread::spawn({
        let (requests, allowed_pids) = (sender.clone(), allowed_pids.clone());
        move || {
            let mut input = std::io::stdin().lock();
            loop {
                match read_one::<Request>(&mut input) {
                    Ok(Some(Request::Allow(pid))) => {
                        if let Ok(mut pids) = allowed_pids.lock() {
                            pids.push(pid);
                        }
                    }
                    Ok(Some(request)) => {
                        if requests.send(Work::Request(request)).is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ = requests.send(Work::Closed);
                        break;
                    }
                }
            }
        }
    });
    std::thread::spawn(move || {
        let pending_count = Arc::new(AtomicUsize::new(0));
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                std::thread::sleep(ACCEPT_RETRY_DELAY);
                continue;
            };
            let Some((pid, uid)) = read_peer_credentials(&stream) else {
                continue;
            };
            if uid != session_uid || pending_count.load(Ordering::SeqCst) >= MAX_PENDING_FORWARDS {
                continue;
            }
            pending_count.fetch_add(1, Ordering::SeqCst);
            let spawn_result = std::thread::Builder::new().spawn({
                let (sender, pending_count, allowed_pids) =
                    (sender.clone(), pending_count.clone(), allowed_pids.clone());
                move || {
                    let _ = stream.set_read_timeout(Some(FORWARD_TIMEOUT));
                    let forward = read_one::<Forward>(&mut stream);
                    pending_count.fetch_sub(1, Ordering::SeqCst);
                    let Ok(Some(forward)) = forward else {
                        return;
                    };
                    let is_trusted = allowed_pids.lock().is_ok_and(|pids| {
                        is_self_or_descendant_of(pid, &pids, read_process_parent)
                    });
                    if !is_trusted {
                        return;
                    }
                    let _ = stream.set_read_timeout(None);
                    let _ = sender.send(Work::Forward(forward, stream));
                }
            });
            if spawn_result.is_err() {
                pending_count.fetch_sub(1, Ordering::SeqCst);
            }
        }
    });

    let mut has_plan = false;
    for work in receiver {
        match work {
            Work::Request(Request::Plan) => {
                has_plan = false;
                match plan() {
                    Ok(()) => has_plan = true,
                    Err(message) => {
                        finish(Err(message));
                    }
                }
            }
            Work::Request(Request::Upgrade(confirmed)) => {
                let result = if has_plan {
                    upgrade(&confirmed)
                } else {
                    Err("the packages to update were not listed first".to_owned())
                };
                has_plan = false;
                finish(result);
                discard_stage();
            }
            Work::Request(Request::Quit) | Work::Closed => break,
            Work::Request(Request::Allow(_)) => {}
            Work::Forward(forward, mut stream) => {
                let exit_code = if tokens_match(&forward.token, &token) {
                    run(&forward.args)
                } else {
                    1
                };
                let _ = stream.write_all(&Exit(exit_code).encode());
            }
        }
    }
    discard_stage();
    Ok(())
}

fn finish(result: Result<(), String>) -> i32 {
    let is_ok = result.is_ok();
    emit(&Event::Done {
        is_ok,
        message: result.err().unwrap_or_default(),
    });
    if is_ok { 0 } else { 1 }
}

fn upgrade(confirmed: &[Change]) -> Result<(), String> {
    let database_path = find_system_database_path()?;
    let stage = database_path.join(STAGE_DIR);
    let mut alpm = open_alpm(None)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Prepare));
    alpm.trans_init(TransFlag::NONE).map_err(format_error)?;
    let result = verify_stage(&stage, confirmed)
        .and_then(|()| {
            install_staged_databases(&stage.join(SYNC_DIR), &database_path.join(SYNC_DIR))
        })
        .and_then(|()| alpm.sync_sysupgrade(false).map_err(format_error))
        .and_then(|()| commit(&mut alpm, Some(confirmed)));
    let _ = alpm.trans_release();
    result
}

fn verify_stage(stage: &Path, confirmed: &[Change]) -> Result<(), String> {
    let mut staged = open_alpm(Some(path_to_str(stage)?))?;
    watch(&staged);
    compare_changes(confirmed, &plan_changes(&mut staged)?)
}

fn commit(alpm: &mut Alpm, confirmed: Option<&[Change]>) -> Result<(), String> {
    alpm.trans_prepare()
        .map_err(|error| format!("{error}: {:?}", error.data()))?;
    if let Some(confirmed) = confirmed {
        compare_changes(confirmed, &collect_changes(alpm))?;
    }
    let package_count = alpm.trans_add().len() as u32;
    let download_bytes = alpm
        .trans_add()
        .iter()
        .map(|package| package.download_size().max(0) as u64)
        .sum();
    emit(&Event::Transaction {
        package_count,
        download_bytes,
    });
    if package_count == 0 && alpm.trans_remove().is_empty() {
        return Ok(());
    }
    alpm.trans_commit()
        .map_err(|error| format!("{error}: {:?}", error.data()))
}

fn install(files: &[&str], flags: TransFlag) -> Result<(), String> {
    let mut alpm = open_alpm(None)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Prepare));
    alpm.trans_init(flags).map_err(format_error)?;
    let mut result = Ok(());
    for file in files {
        let load_result = alpm
            .pkg_load(*file, true, alpm.local_file_siglevel())
            .map_err(|error| format!("{file}: {error}"));
        if let Err(error) = load_result.and_then(|package| {
            alpm.trans_add_pkg(package)
                .map_err(|error| format!("{file}: {}", error.error))
        }) {
            result = Err(error);
            break;
        }
    }
    let result = result.and_then(|()| commit(&mut alpm, None));
    let _ = alpm.trans_release();
    result
}

fn run(args: &[String]) -> i32 {
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

fn parse_native_install(args: &[String]) -> Option<(Vec<&str>, TransFlag)> {
    let mut is_upgrade = false;
    let mut flags = TransFlag::NONE;
    let mut files = Vec::new();
    let mut in_operands = false;
    for argument in args {
        match argument.as_str() {
            _ if in_operands => files.push(argument.as_str()),
            "--" => in_operands = true,
            "-U" | "--upgrade" => is_upgrade = true,
            "--noconfirm" => {}
            "--needed" => flags |= TransFlag::NEEDED,
            "--asdeps" => flags |= TransFlag::ALL_DEPS,
            "--asexplicit" => flags |= TransFlag::ALL_EXPLICIT,
            flag if flag.starts_with('-') => return None,
            file => files.push(file),
        }
    }
    (is_upgrade && !files.is_empty()).then_some((files, flags))
}

fn watch(alpm: &Alpm) {
    let downloads = Rc::new(RefCell::new(Downloads::default()));
    alpm.set_dl_cb(
        downloads.clone(),
        |file: &str, event: AnyDownloadEvent, downloads| {
            let mut downloads = downloads.borrow_mut();
            let entry = downloads.files.entry(file.to_owned()).or_default();
            match event.event() {
                DownloadEvent::Init(_) => *entry = (0, 0),
                DownloadEvent::Progress(progress) => {
                    *entry = (
                        progress.downloaded.max(0) as u64,
                        progress.total.max(0) as u64,
                    );
                }
                DownloadEvent::Completed(completion) => {
                    let total = completion.total.max(0) as u64;
                    *entry = (total, total);
                }
                DownloadEvent::Retry(_) => {}
            }
            let received_bytes: u64 = downloads.files.values().map(|(received, _)| received).sum();
            let file_total_bytes: u64 = downloads.files.values().map(|(_, total)| total).sum();
            let total_bytes = file_total_bytes.max(downloads.expected_bytes);
            let step = (total_bytes / DOWNLOAD_STEPS).max(1);
            if received_bytes < downloads.reported_bytes + step && received_bytes < total_bytes {
                return;
            }
            downloads.reported_bytes = received_bytes;
            emit(&Event::Download {
                received_bytes,
                total_bytes,
            });
        },
    );
    alpm.set_event_cb(downloads, |event: AnyEvent, downloads| {
        match event.event() {
            AlpmEvent::RetrieveStart => *downloads.borrow_mut() = Downloads::default(),
            AlpmEvent::PkgRetrieveStart(retrieval) => {
                *downloads.borrow_mut() = Downloads {
                    expected_bytes: retrieval.total_size().max(0) as u64,
                    ..Downloads::default()
                };
                emit(&Event::Stage(Stage::Download));
            }
            AlpmEvent::TransactionStart => emit(&Event::Stage(Stage::Install)),
            AlpmEvent::HookStart(hook) if hook.when() == HookWhen::PostTransaction => {
                emit(&Event::Stage(Stage::Hooks));
            }
            AlpmEvent::HookRunStart(hook) => emit(&Event::Hook {
                index: hook.position() as u32,
                count: hook.total() as u32,
                name: hook.desc().unwrap_or(hook.name()).to_owned(),
            }),
            AlpmEvent::ScriptletInfo(info) => emit(&Event::Line(info.line().trim_end().to_owned())),
            AlpmEvent::PacnewCreated(pacnew_event) => emit(&Event::Line(format!(
                "{} installed as {}.pacnew",
                pacnew_event.file(),
                pacnew_event.file()
            ))),
            AlpmEvent::PacsaveCreated(pacsave_event) => emit(&Event::Line(format!(
                "{} saved as {}.pacsave",
                pacsave_event.file(),
                pacsave_event.file()
            ))),
            _ => {}
        }
    });
    alpm.set_progress_cb(
        (),
        |progress: Progress, name: &str, percent: i32, count: usize, index: usize, _| {
            if matches!(
                progress,
                Progress::AddStart
                    | Progress::UpgradeStart
                    | Progress::DowngradeStart
                    | Progress::ReinstallStart
                    | Progress::RemoveStart
            ) {
                emit(&Event::Package {
                    index: index as u32,
                    count: count as u32,
                    name: name.to_owned(),
                    percent: percent.clamp(0, 100) as u32,
                });
            }
        },
    );
    alpm.set_log_cb((), |level: LogLevel, message: &str, _| {
        let prefix = if level.contains(LogLevel::ERROR) {
            "error"
        } else if level.contains(LogLevel::WARNING) {
            "warning"
        } else {
            return;
        };
        emit(&Event::Line(format!("{prefix}: {}", message.trim_end())));
    });
    alpm.set_question_cb((), |question: AnyQuestion, _| match question.question() {
        Question::InstallIgnorepkg(mut ignore_question) => ignore_question.set_install(false),
        Question::Replace(replace_question) => replace_question.set_replace(true),
        Question::Conflict(mut conflict_question) => conflict_question.set_remove(false),
        Question::Corrupted(mut corruption_question) => corruption_question.set_remove(true),
        Question::RemovePkgs(mut removal_question) => removal_question.set_skip(false),
        Question::SelectProvider(mut provider_question) => provider_question.set_index(0),
        Question::ImportKey(mut key_question) => key_question.set_import(true),
    });
}

fn forward(args: &[String]) -> i32 {
    let (Ok(name), Some(token)) = (
        std::env::var(SOCKET_VARIABLE),
        std::env::var(TOKEN_VARIABLE)
            .ok()
            .and_then(|token| unhex(&token)),
    ) else {
        eprintln!("proscenio-packages: no update session to forward to");
        return 1;
    };
    let connect_result = SocketAddr::from_abstract_name(name.as_bytes())
        .and_then(|address| UnixStream::connect_addr(&address));
    let Ok(mut stream) = connect_result else {
        eprintln!("proscenio-packages: the update session is gone");
        return 1;
    };
    let request = Forward {
        token,
        args: args.to_vec(),
    };
    if stream.write_all(&request.encode()).is_err() {
        return 1;
    }
    match read_one::<Exit>(&mut stream) {
        Ok(Some(Exit(exit_code))) => exit_code,
        _ => 1,
    }
}

fn relay(stream: impl Read, mut on_line: impl FnMut(String)) {
    for line in BufReader::new(stream).split(b'\n').map_while(Result::ok) {
        let line = line.strip_suffix(b"\r").unwrap_or(&line);
        on_line(String::from_utf8_lossy(line).into_owned());
    }
}

fn read_peer_credentials(stream: &UnixStream) -> Option<(u32, u32)> {
    let mut credentials = Credentials::default();
    let mut length = size_of::<Credentials>() as u32;
    let result = unsafe {
        getsockopt(
            stream.as_raw_fd(),
            SOL_SOCKET,
            SO_PEERCRED,
            (&raw mut credentials).cast(),
            &mut length,
        )
    };
    (result == 0 && credentials.pid > 0).then_some((credentials.pid as u32, credentials.uid))
}

fn read_process_parent(pid: u32) -> Option<u32> {
    parse_parent(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

fn parse_parent(stat: &str) -> Option<u32> {
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn is_self_or_descendant_of(
    pid: u32,
    allowed_pids: &[u32],
    parent: impl Fn(u32) -> Option<u32>,
) -> bool {
    std::iter::successors(Some(pid), |&pid| parent(pid).filter(|&parent| parent != 0))
        .take(MAX_PROCESS_DEPTH)
        .any(|pid| allowed_pids.contains(&pid))
}

fn tokens_match(token: &[u8], expected_token: &[u8]) -> bool {
    if token.len() != expected_token.len() {
        return false;
    }
    let difference =
        token
            .iter()
            .zip(expected_token)
            .fold(0, |difference, (byte, expected_byte)| {
                std::hint::black_box(difference | (byte ^ expected_byte))
            });
    difference == 0
}

fn is_effective_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            let line = status.lines().find(|line| line.starts_with("Uid:"))?;
            line.split_whitespace().nth(2).map(|uid| uid == "0")
        })
        .unwrap_or(false)
}

fn read_random_bytes(count: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0u8; count];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(format_error)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn transactions_run_the_system_hooks_before_the_configured_ones_as_pacman_does() {
        assert_eq!(
            list_hook_dirs(&strings(&["/etc/pacman.d/hooks/"])),
            strings(&[SYSTEM_HOOK_DIR, "/etc/pacman.d/hooks/"])
        );
        assert_eq!(
            list_hook_dirs(&strings(&[SYSTEM_HOOK_DIR, "/etc/pacman.d/hooks/"])),
            strings(&[SYSTEM_HOOK_DIR, "/etc/pacman.d/hooks/"])
        );
    }

    #[test]
    fn pacman_is_forwarded_by_name_as_paru_calls_it_and_by_path_as_makepkg_does() {
        assert!(is_pacman("pacman"));
        assert!(is_pacman("/usr/bin/pacman"));
        assert!(!is_pacman("pacman-key"));
        assert!(!is_pacman("/usr/bin/pacmanx"));
        assert!(!is_pacman("session"));
    }

    #[test]
    fn only_a_plain_upgrade_of_files_is_installed_natively() {
        let paru = strings(&[
            "--upgrade",
            "--noconfirm",
            "--noconfirm",
            "--",
            "/a.pkg",
            "/b.pkg",
        ]);
        assert_eq!(
            parse_native_install(&paru),
            Some((vec!["/a.pkg", "/b.pkg"], TransFlag::NONE))
        );
        let dependencies = strings(&["-U", "--asdeps", "--needed", "/c.pkg"]);
        assert_eq!(
            parse_native_install(&dependencies),
            Some((vec!["/c.pkg"], TransFlag::ALL_DEPS | TransFlag::NEEDED))
        );
        assert_eq!(
            parse_native_install(&strings(&["-S", "--asdeps", "cargo"])),
            None
        );
        assert_eq!(
            parse_native_install(&strings(&["-U", "--overwrite", "*", "/a.pkg"])),
            None
        );
        assert_eq!(parse_native_install(&strings(&["-U"])), None);
        assert_eq!(
            parse_native_install(&strings(&["-D", "--asexplicit", "a"])),
            None
        );
    }

    #[test]
    fn the_parent_follows_the_last_parenthesis_of_the_process_name() {
        assert_eq!(parse_parent("4242 (a) b (c)) S 17 4242 4242 0"), Some(17));
        assert_eq!(parse_parent("1 (systemd) S 0 1 1 0"), Some(0));
        assert_eq!(parse_parent("garbage"), None);
    }

    #[test]
    fn only_a_descendant_of_an_allowed_process_may_forward() {
        let parents = HashMap::from([(30, 20), (20, 10), (10, 1), (40, 1), (1, 0)]);
        let parent = |pid| parents.get(&pid).copied();
        assert!(is_self_or_descendant_of(30, &[20], parent));
        assert!(is_self_or_descendant_of(20, &[20], parent));
        assert!(!is_self_or_descendant_of(40, &[20], parent));
        assert!(!is_self_or_descendant_of(10, &[20], parent));
        assert!(!is_self_or_descendant_of(30, &[], parent));
    }

    #[test]
    fn only_the_session_token_itself_matches() {
        let token = [7, 0, 255, 42];
        assert!(tokens_match(&token, &[7, 0, 255, 42]));
        assert!(!tokens_match(&token, &[7, 0, 255, 43]));
        assert!(!tokens_match(&token, &[8, 0, 255, 42]));
        assert!(!tokens_match(&token, &[7, 0, 255]));
        assert!(!tokens_match(&token, &[7, 0, 255, 42, 0]));
        assert!(!tokens_match(&token, &[]));
    }

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
    fn a_check_holds_its_database_and_clears_a_lock_a_stopped_check_left() {
        let db_path = std::env::temp_dir().join(format!("proscenio-pacman-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&db_path);
        std::fs::create_dir_all(&db_path).unwrap();
        std::fs::write(db_path.join(ALPM_LOCK), "").unwrap();
        let lock_file = lock_check_database(&db_path);
        let has_alpm_lock = db_path.join(ALPM_LOCK).exists();
        let other_file = std::fs::File::open(db_path.join(CHECK_LOCK));
        let is_locked_while_held = other_file
            .as_ref()
            .is_ok_and(|file| file.try_lock().is_err());
        drop(lock_file);
        let free_deadline = std::time::Instant::now() + Duration::from_secs(5);
        let is_free_after = other_file.is_ok_and(|file| {
            while file.try_lock().is_err() {
                if std::time::Instant::now() >= free_deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            true
        });
        std::fs::remove_dir_all(&db_path).unwrap();
        assert!(!has_alpm_lock);
        assert!(is_locked_while_held);
        assert!(is_free_after);
    }

    fn make_scratch_folder(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("proscenio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn only_the_confirmed_packages_may_be_installed() {
        let confirmed = [
            Change::new("linux", "7.2.9-1", "7.2.10-1"),
            Change::new("libfoo", "", "1.0-1"),
        ];
        let reordered = [confirmed[1].clone(), confirmed[0].clone()];
        assert_eq!(compare_changes(&confirmed, &reordered), Ok(()));
        assert_eq!(compare_changes(&[], &[]), Ok(()));

        let extra = [
            confirmed.to_vec(),
            vec![Change::new("glibc", "2.42-1", "2.43-1")],
        ]
        .concat();
        let message = compare_changes(&confirmed, &extra).unwrap_err();
        assert!(message.contains("glibc 2.42-1 -> 2.43-1"), "{message}");
        assert!(message.contains("nothing was installed"), "{message}");

        let missing = compare_changes(&confirmed, &confirmed[..1]).unwrap_err();
        assert!(missing.contains("libfoo"), "{missing}");

        let newer = [
            Change::new("linux", "7.2.9-1", "7.2.11-1"),
            confirmed[1].clone(),
        ];
        let message = compare_changes(&confirmed, &newer).unwrap_err();
        assert!(
            message.contains("7.2.11-1") && message.contains("7.2.10-1"),
            "{message}"
        );

        let removal = [confirmed.to_vec(), vec![Change::new("oldlib", "2.0-1", "")]].concat();
        assert!(compare_changes(&confirmed, &removal).is_err());
    }

    #[test]
    fn a_mismatch_lists_a_few_packages_and_counts_the_rest() {
        let many: Vec<Change> = (0..MAX_LISTED_CHANGES + 3)
            .map(|index| Change::new(&format!("package{index}"), "1-1", "2-1"))
            .collect();
        let message = compare_changes(&[], &many).unwrap_err();
        assert_eq!(message.matches("-> 2-1").count(), MAX_LISTED_CHANGES);
        assert!(message.contains("and 3 more"), "{message}");
    }

    #[test]
    fn a_fresh_stage_shares_the_installed_packages_and_starts_from_the_system_databases() {
        let db_path = make_scratch_folder("stage");
        let stage = db_path.join(STAGE_DIR);
        std::fs::create_dir_all(stage.join(SYNC_DIR)).unwrap();
        std::fs::write(stage.join(SYNC_DIR).join("extra.db"), "earlier stage").unwrap();
        std::fs::write(stage.join(ALPM_LOCK), "").unwrap();
        std::fs::create_dir_all(db_path.join(SYNC_DIR)).unwrap();
        let synced_time = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let system_core = db_path.join(SYNC_DIR).join("core.db");
        std::fs::write(&system_core, "system core").unwrap();
        std::fs::write(db_path.join(SYNC_DIR).join("core.db.sig"), "signature").unwrap();
        std::fs::write(db_path.join(SYNC_DIR).join("core.db.part"), "download").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&system_core)
            .unwrap()
            .set_modified(synced_time)
            .unwrap();

        let prepared = prepare_stage(&db_path);
        let staged_sync = stage.join(SYNC_DIR);
        let mut names: Vec<_> = std::fs::read_dir(&staged_sync)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        let staged_core = staged_sync.join("core.db");
        let content = std::fs::read_to_string(&staged_core).ok();
        let staged_time = std::fs::metadata(&staged_core).and_then(|data| data.modified());
        let local = std::fs::read_link(stage.join(LOCAL_DIR));
        let has_lock = stage.join(ALPM_LOCK).exists();
        std::fs::remove_dir_all(&db_path).unwrap();
        assert_eq!(prepared, Ok(stage));
        assert_eq!(names, ["core.db", "core.db.sig"]);
        assert_eq!(content.as_deref(), Some("system core"));
        assert_eq!(staged_time.ok(), Some(synced_time));
        assert_eq!(local.ok(), Some(db_path.join(LOCAL_DIR)));
        assert!(!has_lock);
    }

    #[test]
    fn staged_databases_replace_the_system_ones_and_nothing_else_moves() {
        let directory = make_scratch_folder("databases");
        let (staged, system) = (directory.join("staged"), directory.join("system"));
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        for (name, content) in [
            ("core.db", "new core"),
            ("core.db.sig", "new signature"),
            ("extra.db.part", "half a download"),
            ("notes.txt", "not a database"),
        ] {
            std::fs::write(staged.join(name), content).unwrap();
        }
        for (name, content) in [("core.db", "old core"), ("extra.db", "old extra")] {
            std::fs::write(system.join(name), content).unwrap();
        }

        let installed = install_staged_databases(&staged, &system);
        let read = |name: &str| std::fs::read_to_string(system.join(name)).ok();
        let results = (
            read("core.db"),
            read("core.db.sig"),
            read("extra.db"),
            read("extra.db.part"),
            read("notes.txt"),
            staged.join("core.db").exists(),
            staged.join("extra.db.part").exists(),
        );
        std::fs::remove_dir_all(&directory).unwrap();
        assert_eq!(installed, Ok(()));
        assert_eq!(
            results,
            (
                Some("new core".to_owned()),
                Some("new signature".to_owned()),
                Some("old extra".to_owned()),
                None,
                None,
                false,
                true,
            )
        );
    }

    #[test]
    fn output_lines_survive_bytes_that_are_not_utf8() {
        let mut lines = Vec::new();
        relay(&b"one\r\n\xe9t\xe9\nthree"[..], |line| lines.push(line));
        assert_eq!(lines, ["one", "\u{fffd}t\u{fffd}", "three"]);
    }
}
