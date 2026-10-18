use proscenio_protocol::{
    Event, Exit, Forward, Message, Request, TOKEN_LENGTH, VERSION, hex, read_one,
};
use std::ffi::{c_int, c_void};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use crate::errors::format_error;
use crate::output::{emit, finish};
use crate::pacman::run;
use crate::stage::discard_stage;
use crate::transaction::upgrade;

const FORWARD_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_RETRY_DELAY: Duration = Duration::from_millis(100);
const MAX_PENDING_FORWARDS: usize = 8;
const MAX_PROCESS_DEPTH: usize = 256;
const SOL_SOCKET: c_int = 1;
const SO_PEERCRED: c_int = 17;

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

pub fn run_session() -> Result<(), String> {
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

    for work in receiver {
        match work {
            Work::Request(Request::Upgrade {
                changes,
                private_path,
            }) => {
                finish(upgrade(&changes, Path::new(&private_path), session_uid));
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
    use std::collections::HashMap;

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
}
