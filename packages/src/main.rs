mod changes;
mod check;
mod errors;
mod export;
mod files;
mod forwarding;
mod handle;
mod headers;
mod hyprpm;
mod output;
mod package_cache;
mod pacman;
mod plan;
mod private_database;
mod session;
mod stage;
mod transaction;

use check::check;
use export::export;
use forwarding::{forward, is_pacman, is_run0};
use handle::{USER_AGENT_VARIABLE, describe_user_agent};
use hyprpm::run_hyprpm_session;
use plan::plan;
use proscenio_protocol::ERROR_PREFIX;
use session::run_session;

fn main() {
    if std::env::var_os(USER_AGENT_VARIABLE).is_none_or(|user_agent| user_agent.is_empty()) {
        unsafe { std::env::set_var(USER_AGENT_VARIABLE, describe_user_agent()) };
    }
    let program = std::env::args().next().unwrap_or_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if is_run0(&program) {
        std::process::exit(forward(&args));
    }
    let exit_code = match args.first().map(String::as_str) {
        Some("check") if args.len() == 2 => report(check(&args[1])),
        Some("plan") if args.len() == 2 => report(plan(&args[1])),
        Some("session") if args.len() == 1 => report(run_session()),
        Some("hyprpm") if args.len() == 1 => report(run_hyprpm_session()),
        Some("export") if args.len() == 2 => report(export(&args[1])),
        Some(program) if is_pacman(program) => forward(&args[1..]),
        _ => {
            eprintln!(
                "usage: proscenio-packages check DBPATH | plan DBPATH | session | hyprpm | export PATH | pacman ARGUMENT…"
            );
            2
        }
    };
    std::process::exit(exit_code);
}

fn report(result: Result<(), String>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("{ERROR_PREFIX}{message}");
            1
        }
    }
}
