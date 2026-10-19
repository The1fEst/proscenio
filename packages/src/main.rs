mod changes;
mod check;
mod errors;
mod forwarding;
mod handle;
mod output;
mod package_cache;
mod pacman;
mod plan;
mod private_database;
mod session;
mod stage;
mod transaction;

use check::check;
use forwarding::{forward, is_pacman};
use plan::plan;
use session::run_session;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exit_code = match args.first().map(String::as_str) {
        Some("check") if args.len() == 2 => report(check(&args[1])),
        Some("plan") if args.len() == 2 => report(plan(&args[1])),
        Some("session") if args.len() == 1 => report(run_session()),
        Some(program) if is_pacman(program) => forward(&args[1..]),
        _ => {
            eprintln!(
                "usage: proscenio-packages check DBPATH | plan DBPATH | session | pacman ARGUMENT…"
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
            eprintln!("proscenio-packages: {message}");
            1
        }
    }
}
