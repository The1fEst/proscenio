use gtk4::glib;
use std::path::Path;

use crate::core::config;
use crate::core::tools;
use crate::platform::system::desktop::{self, DesktopEntry};

pub const POINTER: &str = "/apps/terminal";
pub const DEFAULT_COMMAND: &str = "kitty -1";

pub struct Terminal {
    pub name: String,
    pub command: String,
}

pub struct Choices {
    pub terminals: Vec<Terminal>,
    pub current_index: usize,
}

fn parse_program_name(command: &str) -> Option<&str> {
    Path::new(tools::command_program(command)?)
        .file_name()?
        .to_str()
}

fn is_terminal_emulator(categories: &str) -> bool {
    categories
        .split(';')
        .any(|category| category == "TerminalEmulator")
}

fn join_arguments(arguments: &[String]) -> String {
    let words: Vec<String> = arguments
        .iter()
        .map(|argument| {
            if argument.chars().all(is_plain) {
                argument.clone()
            } else {
                glib::shell_quote(argument).to_string_lossy().into_owned()
            }
        })
        .collect();
    words.join(" ")
}

fn is_plain(character: char) -> bool {
    character.is_ascii_alphanumeric() || "_-./=:@+,%".contains(character)
}

fn has_same_program(first: &str, second: &str) -> bool {
    parse_program_name(first).is_some_and(|program| Some(program) == parse_program_name(second))
}

pub fn with_current(mut terminals: Vec<Terminal>, stored: &str) -> Choices {
    let current_index = terminals
        .iter()
        .position(|terminal| has_same_program(&terminal.command, stored))
        .unwrap_or_else(|| {
            terminals.push(Terminal {
                name: stored.to_owned(),
                command: stored.to_owned(),
            });
            terminals.len() - 1
        });
    Choices {
        terminals,
        current_index,
    }
}

pub fn resolve_command(terminal: &Terminal, stored: &str) -> String {
    if has_same_program(&terminal.command, stored) {
        stored.to_owned()
    } else {
        terminal.command.clone()
    }
}

pub fn read_command() -> String {
    config::value_str(POINTER)
        .filter(|command| !command.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_COMMAND.to_owned())
}

pub fn list_installed() -> Vec<Terminal> {
    let mut terminals: Vec<Terminal> = desktop::all()
        .into_iter()
        .filter(DesktopEntry::is_shown)
        .filter(|entry| {
            entry
                .string("Categories")
                .is_some_and(|categories| is_terminal_emulator(&categories))
        })
        .filter_map(|entry| {
            let arguments = entry.command();
            if arguments.is_empty() {
                return None;
            }
            Some(Terminal {
                name: entry.name(),
                command: join_arguments(&arguments),
            })
        })
        .collect();
    terminals.sort_by_key(|terminal| terminal.name.to_lowercase());
    terminals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal(name: &str, command: &str) -> Terminal {
        Terminal {
            name: name.to_owned(),
            command: command.to_owned(),
        }
    }

    fn listed_terminals() -> Vec<Terminal> {
        vec![
            terminal("Alacritty", "alacritty"),
            terminal("foot", "foot"),
            terminal("kitty", "kitty"),
        ]
    }

    #[test]
    fn the_stored_command_selects_the_terminal_with_the_same_program() {
        for (stored, expected) in [
            ("kitty -1", 2),
            ("/usr/bin/foot --server", 1),
            ("GDK_SCALE=2 alacritty", 0),
        ] {
            assert_eq!(
                with_current(listed_terminals(), stored).current_index,
                expected
            );
        }
    }

    #[test]
    fn a_stored_command_without_a_listed_terminal_is_added_and_selected() {
        let choices = with_current(listed_terminals(), "wezterm start");
        assert_eq!(choices.current_index, 3);
        assert_eq!(choices.terminals.len(), 4);
        assert_eq!(choices.terminals[3].name, "wezterm start");
        assert_eq!(choices.terminals[3].command, "wezterm start");
    }

    #[test]
    fn a_stored_command_keeps_its_flags_when_the_same_program_is_chosen() {
        let kitty = terminal("kitty", "kitty");
        assert_eq!(resolve_command(&kitty, "kitty -1"), "kitty -1");
        assert_eq!(
            resolve_command(&kitty, "/usr/bin/kitty --single-instance"),
            "/usr/bin/kitty --single-instance"
        );
    }

    #[test]
    fn choosing_another_program_stores_its_command() {
        assert_eq!(
            resolve_command(&terminal("foot", "foot"), "kitty -1"),
            "foot"
        );
        assert_eq!(
            resolve_command(&terminal("foot", "foot --server"), "kitty -1"),
            "foot --server"
        );
    }

    #[test]
    fn only_entries_in_the_terminal_emulator_category_are_terminals() {
        assert!(is_terminal_emulator("System;TerminalEmulator;"));
        assert!(is_terminal_emulator("TerminalEmulator"));
        assert!(!is_terminal_emulator("System;Utility;"));
        assert!(!is_terminal_emulator("TerminalEmulatorPlugin;"));
        assert!(!is_terminal_emulator(""));
    }

    #[test]
    fn an_argument_with_spaces_stays_one_word_of_the_command() {
        assert_eq!(
            join_arguments(&["term".to_owned(), "--title=My shell".to_owned()]),
            "term '--title=My shell'"
        );
        assert_eq!(
            join_arguments(&["term".to_owned(), "--class=app".to_owned()]),
            "term --class=app"
        );
    }
}
