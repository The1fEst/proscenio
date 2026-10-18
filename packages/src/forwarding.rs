use proscenio_protocol::{
    Exit, Forward, Message, SOCKET_VARIABLE, TOKEN_VARIABLE, read_one, unhex,
};
use std::io::Write;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};

pub fn is_pacman(program: &str) -> bool {
    program == "pacman" || program.ends_with("/pacman")
}

pub fn forward(args: &[String]) -> i32 {
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
}
