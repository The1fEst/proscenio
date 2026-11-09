use proscenio_protocol::{
    ERROR_PREFIX, Exit, Forward, Message, SOCKET_VARIABLE, TOKEN_VARIABLE, read_one, unhex,
};
use std::io::Write;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};

fn is_program(program: &str, name: &str) -> bool {
    program.rsplit('/').next() == Some(name)
}

pub fn is_pacman(program: &str) -> bool {
    is_program(program, "pacman")
}

pub fn is_run0(program: &str) -> bool {
    is_program(program, "run0")
}

pub fn forward(args: &[String]) -> i32 {
    let (Ok(name), Some(token)) = (
        std::env::var(SOCKET_VARIABLE),
        std::env::var(TOKEN_VARIABLE)
            .ok()
            .and_then(|token| unhex(&token)),
    ) else {
        eprintln!("{ERROR_PREFIX}no update session to forward to");
        return 1;
    };
    let connect_result = SocketAddr::from_abstract_name(name.as_bytes())
        .and_then(|address| UnixStream::connect_addr(&address));
    let Ok(mut stream) = connect_result else {
        eprintln!("{ERROR_PREFIX}the update session is gone");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacman_is_forwarded_by_name_as_paru_calls_it_and_by_path_as_makepkg_does() {
        assert!(is_pacman("pacman"));
        assert!(is_pacman("/usr/bin/pacman"));
        assert!(!is_pacman("pacman-key"));
        assert!(!is_pacman("/usr/bin/pacmanx"));
        assert!(!is_pacman("session"));
    }

    #[test]
    fn the_helper_forwards_when_hyprpm_runs_it_as_run0() {
        assert!(is_run0("run0"));
        assert!(is_run0("/home/user/.cache/proscenio/hyprpm-tools/run0"));
        assert!(!is_run0("/usr/lib/proscenio/proscenio-packages"));
        assert!(!is_run0("xrun0x"));
        assert!(!is_run0("srun0"));
    }
}
