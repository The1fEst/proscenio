use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

use super::NO_TIMEOUT;
use super::ipc::{INTERFACE, PATH};
use crate::core::dmenu::{Reply, Request, Responder};

pub const COMMAND: &str = "dmenu";
pub const METHOD: &str = "Dmenu";
const USAGE: &str =
    "usage: proscenio dmenu [-p PROMPT] [-l LINES] [--placeholder TEXT] [-i] [--prompt-only]\n";
const NO_INSTANCE_MESSAGE: &str = "No running instance of proscenio.";
const OUTDATED_INSTANCE_MESSAGE: &str =
    "The running proscenio does not know dmenu; restart the shell.";

pub type Handler = Rc<dyn Fn(Request, Responder)>;

#[derive(Debug, Default, PartialEq)]
struct Options {
    prompt: String,
    placeholder: String,
    lines: u32,
    is_prompt_only: bool,
}

fn to_exit_status(answer: &Result<Reply, String>) -> u8 {
    match answer {
        Ok(Reply::Choice(_)) => 0,
        Ok(Reply::Cancellation) => 1,
        Err(_) => 2,
    }
}

pub fn run(bus_name: &str, arguments: &[String]) -> glib::ExitCode {
    let options = match parse(arguments) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprint!("{USAGE}");
            return glib::ExitCode::FAILURE;
        }
    };
    let mut items = Vec::new();
    if !options.is_prompt_only {
        let mut input = Vec::new();
        if let Err(error) = std::io::stdin().lock().read_to_end(&mut input) {
            eprintln!("Cannot read standard input: {error}");
            return glib::ExitCode::FAILURE;
        }
        items = split_items(&input);
        if items.is_empty() {
            return glib::ExitCode::from(to_exit_status(&Ok(Reply::Cancellation)));
        }
    }
    let answer = ask(bus_name, &options, items);
    match &answer {
        Ok(Reply::Choice(choice)) => {
            let _ = writeln!(std::io::stdout(), "{choice}");
        }
        Ok(Reply::Cancellation) => {}
        Err(message) => eprintln!("{message}"),
    }
    glib::ExitCode::from(to_exit_status(&answer))
}

pub fn serve(
    handler: Option<Handler>,
    connection: &gio::DBusConnection,
    sender: Option<&str>,
    parameters: &Variant,
    invocation: gio::DBusMethodInvocation,
) {
    let (Some(handler), Some(request)) = (handler, read_request(parameters)) else {
        invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.InvalidArgs",
            "The shell cannot take this dmenu request.",
        );
        return;
    };
    let watcher = Rc::new(RefCell::new(None));
    let responder = Responder::new({
        let watcher = watcher.clone();
        move |reply| {
            if let Some(watcher) = watcher.take() {
                gio::bus_unwatch_name(watcher);
            }
            invocation.return_value(Some(&write_reply(&reply)));
        }
    });
    if let Some(sender) = sender {
        let on_abandon = responder.on_abandon();
        watcher.replace(Some(gio::bus_watch_name_on_connection(
            connection,
            sender,
            gio::BusNameWatcherFlags::NONE,
            |_, _, _| {},
            move |_, _| on_abandon.invoke(),
        )));
    }
    handler(request, responder);
}

fn ask(bus_name: &str, options: &Options, items: Vec<String>) -> Result<Reply, String> {
    let parameters = write_request(options, items);
    let reply =
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).and_then(|connection| {
            connection.call_sync(
                Some(bus_name),
                PATH,
                INTERFACE,
                METHOD,
                Some(&parameters),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                NO_TIMEOUT,
                gio::Cancellable::NONE,
            )
        });
    read_reply(reply)
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        let (name, inline_value) = match argument.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value)),
            _ => (argument.as_str(), None),
        };
        match name {
            "-i" | "--prompt-only" if inline_value.is_some() => {
                return Err(format!("{name} takes no value"));
            }
            "-i" => {}
            "--prompt-only" => options.is_prompt_only = true,
            "-p" | "--prompt" => options.prompt = take_value(name, inline_value, &mut rest)?,
            "--placeholder" => options.placeholder = take_value(name, inline_value, &mut rest)?,
            "-l" | "--lines" => {
                let text = take_value(name, inline_value, &mut rest)?;
                options.lines = text
                    .parse()
                    .map_err(|_| format!("Invalid number of lines: {text}"))?;
            }
            _ => return Err(format!("Unknown option: {argument}")),
        }
    }
    Ok(options)
}

fn take_value<'a>(
    name: &str,
    inline_value: Option<&str>,
    rest: &mut impl Iterator<Item = &'a String>,
) -> Result<String, String> {
    match inline_value {
        Some(value) => Ok(value.to_owned()),
        None => rest
            .next()
            .cloned()
            .ok_or_else(|| format!("{name} needs a value")),
    }
}

fn split_items(input: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(input).replace('\0', "\u{FFFD}");
    let mut items: Vec<String> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect();
    if items.last().is_some_and(String::is_empty) {
        items.pop();
    }
    items
}

fn write_request(options: &Options, items: Vec<String>) -> Variant {
    (
        options.prompt.as_str(),
        options.placeholder.as_str(),
        options.lines,
        options.is_prompt_only,
        items,
    )
        .to_variant()
}

fn read_request(parameters: &Variant) -> Option<Request> {
    let (prompt, placeholder, lines, is_prompt_only, items) =
        parameters.get::<(String, String, u32, bool, Vec<String>)>()?;
    Some(Request {
        prompt,
        placeholder,
        lines,
        is_prompt_only,
        items,
    })
}

fn write_reply(reply: &Reply) -> Variant {
    let (is_accepted, choice) = match reply {
        Reply::Choice(choice) => (true, choice.as_str()),
        Reply::Cancellation => (false, ""),
    };
    (is_accepted, choice).to_variant()
}

fn read_reply(reply: Result<Variant, glib::Error>) -> Result<Reply, String> {
    match reply {
        Ok(reply) => match reply.get::<(bool, String)>() {
            Some((true, choice)) => Ok(Reply::Choice(choice)),
            _ => Ok(Reply::Cancellation),
        },
        Err(error)
            if error.matches(gio::DBusError::ServiceUnknown)
                || error.matches(gio::DBusError::NameHasNoOwner) =>
        {
            Err(NO_INSTANCE_MESSAGE.to_owned())
        }
        Err(error) if error.matches(gio::DBusError::UnknownMethod) => {
            Err(OUTDATED_INSTANCE_MESSAGE.to_owned())
        }
        Err(mut error) => {
            gio::DBusError::strip_remote_error(&mut error);
            Err(error.message().to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_arguments(list: &[&str]) -> Vec<String> {
        list.iter().map(|argument| (*argument).to_owned()).collect()
    }

    #[test]
    fn the_arguments_set_the_prompt_the_placeholder_the_lines_and_the_prompt_only_flag() {
        assert_eq!(parse(&[]), Ok(Options::default()));
        let expected_options = Options {
            prompt: "Clipboard".to_owned(),
            placeholder: "Type to filter".to_owned(),
            lines: 12,
            is_prompt_only: true,
        };
        let spaced_arguments = [
            "-p",
            "Clipboard",
            "-l",
            "12",
            "--placeholder",
            "Type to filter",
            "-i",
            "--prompt-only",
        ];
        assert_eq!(
            parse(&build_arguments(&spaced_arguments)),
            Ok(expected_options)
        );
        let joined_arguments = [
            "--prompt=Clipboard",
            "--lines=12",
            "--placeholder=Type to filter",
            "--prompt-only",
        ];
        assert_eq!(
            parse(&build_arguments(&joined_arguments)),
            Ok(Options {
                prompt: "Clipboard".to_owned(),
                placeholder: "Type to filter".to_owned(),
                lines: 12,
                is_prompt_only: true,
            })
        );
    }

    #[test]
    fn a_value_may_contain_an_equals_sign_and_start_with_a_dash() {
        let parsed_options = parse(&build_arguments(&["-p", "-a=b", "--placeholder=x=y"])).unwrap();
        assert_eq!(parsed_options.prompt, "-a=b");
        assert_eq!(parsed_options.placeholder, "x=y");
    }

    #[test]
    fn a_bad_command_line_is_refused() {
        for bad in [
            &["--unknown"][..],
            &["positional"],
            &["-p"],
            &["--lines"],
            &["-l", "many"],
            &["-l", "-3"],
            &["--prompt-only=yes"],
            &["-i=yes"],
        ] {
            assert!(parse(&build_arguments(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn standard_input_splits_into_lines_without_the_trailing_empty_one() {
        assert_eq!(split_items(b"one\ntwo\n"), ["one", "two"]);
        assert_eq!(split_items(b"one\ntwo"), ["one", "two"]);
        assert_eq!(split_items(b"one\r\ntwo\r\n"), ["one", "two"]);
        assert_eq!(split_items(b"one\n\ntwo\n"), ["one", "", "two"]);
        assert_eq!(split_items(b"one\n\n"), ["one", ""]);
        assert_eq!(split_items("ё\u{1F600}\n".as_bytes()), ["ё\u{1F600}"]);
        assert_eq!(split_items(b"bad\xffbyte\n"), ["bad\u{FFFD}byte"]);
        assert_eq!(split_items(b"nul\0byte\n"), ["nul\u{FFFD}byte"]);
    }

    #[test]
    fn empty_standard_input_has_no_items_and_a_lone_newline_is_one_blank_item() {
        assert!(split_items(b"").is_empty());
        assert_eq!(split_items(b"\n"), [""]);
    }

    #[test]
    fn the_exit_status_tells_a_choice_a_cancel_and_a_missing_shell_apart() {
        assert_eq!(to_exit_status(&Ok(Reply::Choice("a".to_owned()))), 0);
        assert_eq!(to_exit_status(&Ok(Reply::Choice(String::new()))), 0);
        assert_eq!(to_exit_status(&Ok(Reply::Cancellation)), 1);
        assert_eq!(to_exit_status(&Err(String::new())), 2);
    }

    #[test]
    fn a_reply_crosses_the_bus_whole() {
        let choice = write_reply(&Reply::Choice("line".to_owned()));
        assert_eq!(read_reply(Ok(choice)), Ok(Reply::Choice("line".to_owned())));
        let empty_choice = write_reply(&Reply::Choice(String::new()));
        assert_eq!(
            read_reply(Ok(empty_choice)),
            Ok(Reply::Choice(String::new()))
        );
        let cancellation = write_reply(&Reply::Cancellation);
        assert_eq!(read_reply(Ok(cancellation)), Ok(Reply::Cancellation));
    }

    #[test]
    fn a_bus_without_the_shell_is_a_missing_shell_and_other_errors_keep_their_message() {
        let missing_error = glib::Error::new(gio::DBusError::ServiceUnknown, "gone");
        assert_eq!(
            read_reply(Err(missing_error)),
            Err(NO_INSTANCE_MESSAGE.to_owned())
        );
        let unowned_error = glib::Error::new(gio::DBusError::NameHasNoOwner, "gone");
        assert_eq!(
            read_reply(Err(unowned_error)),
            Err(NO_INSTANCE_MESSAGE.to_owned())
        );
        let outdated_error = glib::Error::new(gio::DBusError::UnknownMethod, "No such method");
        assert_eq!(
            read_reply(Err(outdated_error)),
            Err(OUTDATED_INSTANCE_MESSAGE.to_owned())
        );
        let other_error = glib::Error::new(gio::DBusError::LimitsExceeded, "too big");
        assert_eq!(read_reply(Err(other_error)), Err("too big".to_owned()));
    }

    #[test]
    fn a_request_crosses_the_bus_whole() {
        let options = Options {
            prompt: "Pick".to_owned(),
            placeholder: "Type".to_owned(),
            lines: 7,
            is_prompt_only: false,
        };
        let items = vec!["one".to_owned(), String::new(), "ё".to_owned()];
        let request = read_request(&write_request(&options, items.clone())).unwrap();
        assert_eq!(request.prompt, "Pick");
        assert_eq!(request.placeholder, "Type");
        assert_eq!(request.lines, 7);
        assert!(!request.is_prompt_only);
        assert_eq!(request.items, items);
        assert!(read_request(&"not a request".to_variant()).is_none());
    }
}
