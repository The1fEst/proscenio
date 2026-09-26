use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::config::{self, Config};
use crate::core::fuzzy::{self, Prepared};
use crate::core::levenshtein;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::detach;
use crate::platform::desktop::{self, DesktopAction, DesktopEntry};
use crate::services::cliphist::{self, Cliphist};
use crate::services::net::Net;
use crate::services::todo::Todo;

const EMOJI_DATA: &str = "### DATA ###";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconType {
    Material,
    Text,
    System,
    None,
}

#[derive(Clone)]
pub struct Item {
    pub kind: String,
    pub monospace: bool,
    pub name: String,
    pub raw: String,
    pub icon: String,
    pub icon_type: IconType,
    pub verb: String,
    pub blur: bool,
    pub run: Run,
    pub actions: Vec<Item>,
}

impl Item {
    fn new(name: &str, run: Run) -> Self {
        Item {
            kind: String::new(),
            monospace: false,
            name: name.to_owned(),
            raw: String::new(),
            icon: String::new(),
            icon_type: IconType::None,
            verb: String::new(),
            blur: false,
            run,
            actions: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub enum Run {
    App(Rc<App>),
    AppAction(Rc<App>, usize),
    Shell(String),
    Clipboard(String),
    CopyEntry(String),
    DeleteEntry(String),
    Builtin(&'static str, String),
    Script(String, String),
}

struct Emojis {
    lines: Vec<String>,
    names: Vec<Prepared>,
}

pub struct App {
    entry: DesktopEntry,
    name: String,
    icon: String,
    actions: Vec<DesktopAction>,
}

#[derive(Clone)]
pub struct Launcher {
    pub query: Rc<RefCell<String>>,
    math: Rc<RefCell<String>>,
    math_timer: Rc<RefCell<Option<glib::SourceId>>>,
    math_generation: Rc<Cell<u64>>,
    apps: Rc<RefCell<Vec<Rc<App>>>>,
    app_names: Rc<RefCell<Vec<Prepared>>>,
    emojis: Rc<RefCell<Option<Rc<Emojis>>>>,
    cliphist: Cliphist,
    todo: Todo,
    net: Net,
    listeners: Rc<Listeners>,
}

impl Launcher {
    pub fn new(cliphist: &Cliphist, todo: &Todo, net: &Net) -> Self {
        let launcher = Launcher {
            query: Rc::new(RefCell::new(String::new())),
            math: Rc::new(RefCell::new(String::new())),
            math_timer: Rc::new(RefCell::new(None)),
            math_generation: Rc::new(Cell::new(0)),
            apps: Rc::new(RefCell::new(Vec::new())),
            app_names: Rc::new(RefCell::new(Vec::new())),
            emojis: Rc::new(RefCell::new(None)),
            cliphist: cliphist.clone(),
            todo: todo.clone(),
            net: net.clone(),
            listeners: Rc::new(Listeners::default()),
        };
        cliphist
            .subscribe({
                let launcher = launcher.clone();
                move || launcher.announce()
            })
            .forever();
        launcher
    }

    fn config(&self) -> Rc<Config> {
        config::current()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    fn announce(&self) {
        self.listeners.notify();
    }

    pub fn load(&self) {
        self.reload_apps();
        self.cliphist.load();
    }

    pub fn release(&self) {
        self.apps.replace(Vec::new());
        self.app_names.replace(Vec::new());
        self.emojis.replace(None);
        self.math.replace(String::new());
        self.cliphist.release();
    }

    fn emojis(&self) -> Rc<Emojis> {
        if let Some(emojis) = self.emojis.borrow().as_ref() {
            return emojis.clone();
        }
        let lines = load_emojis();
        let names = lines.iter().map(|line| fuzzy::prepare(line)).collect();
        let emojis = Rc::new(Emojis { lines, names });
        self.emojis.replace(Some(emojis.clone()));
        emojis
    }

    fn reload_apps(&self) {
        let apps: Vec<Rc<App>> = desktop::all()
            .into_iter()
            .filter(DesktopEntry::shown)
            .map(|entry| {
                Rc::new(App {
                    name: entry.name(),
                    icon: entry.icon(),
                    actions: entry.actions(),
                    entry,
                })
            })
            .collect();
        let names = apps
            .iter()
            .map(|app| fuzzy::prepare(&format!("{} ", app.name)))
            .collect();
        self.apps.replace(apps);
        self.app_names.replace(names);
    }

    pub fn set_query(&self, query: &str) {
        if *self.query.borrow() == query {
            return;
        }
        self.query.replace(query.to_owned());
        let special = query.is_empty()
            || query.starts_with(&self.config().search_clipboard)
            || query.starts_with(&self.config().search_emojis);
        if !special {
            self.restart_math_timer();
        }
        self.announce();
    }

    pub fn clean_one_prefix<'a>(&self, text: &'a str) -> &'a str {
        let config = self.config();
        for prefix in [
            &config.search_action,
            &config.search_app,
            &config.search_clipboard,
            &config.search_emojis,
            &config.search_math,
            &config.search_shell,
        ] {
            if let Some(rest) = text.strip_prefix(prefix.as_str()) {
                return rest;
            }
        }
        text
    }

    fn restart_math_timer(&self) {
        if let Some(source) = self.math_timer.borrow_mut().take() {
            source.remove();
        }
        let launcher = self.clone();
        let source = glib::timeout_add_local_once(
            Duration::from_millis(self.config().search_non_app_delay),
            move || {
                launcher.math_timer.borrow_mut().take();
                let query = launcher.query.borrow().clone();
                let expression = query
                    .strip_prefix(launcher.config().search_math.as_str())
                    .unwrap_or(&query)
                    .to_owned();
                launcher.calculate(expression);
            },
        );
        self.math_timer.replace(Some(source));
    }

    fn calculate(&self, expression: String) {
        let generation = self.math_generation.get() + 1;
        self.math_generation.set(generation);
        let launcher = self.clone();
        glib::spawn_future_local(async move {
            let Some(output) = read(&["qalc", "-t", &expression]).await else {
                return;
            };
            if launcher.math_generation.get() != generation {
                return;
            }
            let Some(answer) = output.lines().filter(|line| !line.is_empty()).last() else {
                return;
            };
            if *launcher.math.borrow() == answer {
                return;
            }
            launcher.math.replace(answer.to_owned());
            launcher.announce();
        });
    }

    pub fn results(&self) -> Vec<Item> {
        let query = self.query.borrow().clone();
        let config = self.config();
        if query.is_empty() {
            return Vec::new();
        }

        if let Some(search) = query.strip_prefix(config.search_clipboard.as_str()) {
            return self.clipboard_results(search);
        }
        if let Some(search) = query.strip_prefix(config.search_emojis.as_str()) {
            return self.emoji_results(search);
        }

        let answer = self.math.borrow().clone();
        let math = Item {
            kind: "Math result".to_owned(),
            monospace: true,
            icon: "calculate".to_owned(),
            icon_type: IconType::Material,
            verb: "Copy".to_owned(),
            ..Item::new(&answer, Run::Clipboard(answer.clone()))
        };
        let command_name = query
            .strip_prefix(config.search_shell.as_str())
            .unwrap_or(&query)
            .replace("file://", "");
        let command = Item {
            kind: "Command".to_owned(),
            monospace: true,
            icon: "terminal".to_owned(),
            icon_type: IconType::Material,
            verb: "Run".to_owned(),
            ..Item::new(&command_name, Run::Shell(self.command_line(&query)))
        };

        let apps = self.app_results(
            query
                .strip_prefix(config.search_app.as_str())
                .unwrap_or(&query),
        );
        let actions = self.action_results(&query);

        let starts_with_number = query.starts_with(|letter: char| letter.is_ascii_digit());
        let math_prefix = query.starts_with(config.search_math.as_str());
        let shell_prefix = query.starts_with(config.search_shell.as_str());

        let mut results = Vec::new();
        if starts_with_number || math_prefix {
            results.push(math.clone());
        } else if shell_prefix {
            results.push(command.clone());
        }
        results.extend(apps);
        results.extend(actions);
        if config.search_default_actions {
            if !shell_prefix {
                results.push(command);
            }
            if !starts_with_number && !math_prefix {
                results.push(math);
            }
        }
        results
    }

    fn command_line(&self, query: &str) -> String {
        let config = self.config();
        let shell = config.search_shell.as_str();
        let cleaned = query.replace("file://", "");
        let cleaned = cleaned.strip_prefix(shell).unwrap_or(&cleaned);
        let cleaned = cleaned.strip_prefix(shell).unwrap_or(cleaned);
        if query.starts_with("sudo") {
            return format!("{} fish -C '{cleaned}'", self.config().app_terminal);
        }
        cleaned.to_owned()
    }

    fn app_results(&self, search: &str) -> Vec<Item> {
        let apps = self.apps.borrow();
        let found = if levenshtein::sloppy() {
            let names = apps.iter().map(|app| app.name.as_str());
            levenshtein::rank(names, search, levenshtein::score)
        } else {
            let names = self.app_names.borrow();
            let references: Vec<&Prepared> = names.iter().collect();
            fuzzy::go(search, &references)
        };
        found
            .into_iter()
            .filter_map(|index| apps.get(index).cloned())
            .map(|app| {
                let actions = app
                    .actions
                    .iter()
                    .enumerate()
                    .map(|(index, action)| Item {
                        icon: action.icon.clone(),
                        icon_type: IconType::System,
                        ..Item::new(&action.name, Run::AppAction(app.clone(), index))
                    })
                    .collect();
                Item {
                    kind: "App".to_owned(),
                    icon: app.icon.clone(),
                    icon_type: IconType::System,
                    verb: "Open".to_owned(),
                    actions,
                    ..Item::new(&app.name, Run::App(app.clone()))
                }
            })
            .collect()
    }

    fn clipboard_results(&self, search: &str) -> Vec<Item> {
        let entries = self.cliphist.fuzzy_query(search);
        let guarded = self.clipboard_guarded();
        let config = self.config();
        let unsafe_link = |entry: Option<&String>| {
            entry.is_some_and(|entry| {
                let lower = entry.to_lowercase();
                config
                    .safety_links
                    .iter()
                    .any(|keyword| lower.contains(keyword.as_str()))
            })
        };
        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let blur = guarded
                    && cliphist::is_image(entry)
                    && (unsafe_link(index.checked_sub(1).and_then(|before| entries.get(before)))
                        || unsafe_link(entries.get(index + 1)));
                let first = entry.split_whitespace().next().unwrap_or("");
                Item {
                    kind: format!("#{first}"),
                    raw: entry.clone(),
                    blur,
                    actions: vec![
                        Item {
                            icon: "content_copy".to_owned(),
                            icon_type: IconType::Material,
                            ..Item::new("Copy", Run::CopyEntry(entry.clone()))
                        },
                        Item {
                            icon: "delete".to_owned(),
                            icon_type: IconType::Material,
                            ..Item::new("Delete", Run::DeleteEntry(entry.clone()))
                        },
                    ],
                    ..Item::new(cliphist::clean(entry), Run::CopyEntry(entry.clone()))
                }
            })
            .collect()
    }

    fn clipboard_guarded(&self) -> bool {
        let config = self.config();
        if !config.safety_clipboard {
            return false;
        }
        let network = self.net.name.borrow().to_lowercase();
        config
            .safety_networks
            .iter()
            .any(|keyword| network.contains(keyword.as_str()))
    }

    fn emoji_results(&self, search: &str) -> Vec<Item> {
        let emojis = self.emojis();
        let references: Vec<&Prepared> = emojis.names.iter().collect();
        let found: Vec<usize> = if search.is_empty() {
            (0..emojis.lines.len()).collect()
        } else if levenshtein::sloppy() {
            let lines = emojis.lines.iter().map(String::as_str);
            levenshtein::rank(lines, search, levenshtein::text_match_score)
        } else {
            fuzzy::go(search, &references)
        };
        found
            .into_iter()
            .filter_map(|index| emojis.lines.get(index))
            .map(|line| {
                let emoji = line.split_whitespace().next().unwrap_or("").to_owned();
                Item {
                    kind: "Emoji".to_owned(),
                    raw: line.clone(),
                    icon: emoji.clone(),
                    icon_type: IconType::Text,
                    verb: "Copy".to_owned(),
                    ..Item::new(cliphist::without_first_word(line), Run::Clipboard(emoji))
                }
            })
            .collect()
    }

    fn action_results(&self, query: &str) -> Vec<Item> {
        let arguments = query.split(' ').skip(1).collect::<Vec<_>>().join(" ");
        let mut named: Vec<(String, Run)> = BUILTINS
            .iter()
            .map(|name| (name.to_string(), Run::Builtin(name, arguments.clone())))
            .collect();
        named.extend(
            scripts()
                .into_iter()
                .map(|(name, path)| (name, Run::Script(path, arguments.clone()))),
        );
        named
            .into_iter()
            .filter_map(|(name, run)| {
                let full = format!("{}{name}", self.config().search_action);
                if !full.starts_with(query) && !query.starts_with(&full) {
                    return None;
                }
                let shown = if query.starts_with(&full) {
                    query
                } else {
                    &full
                };
                Some(Item {
                    kind: "Action".to_owned(),
                    icon: "settings_suggest".to_owned(),
                    icon_type: IconType::Material,
                    verb: "Run".to_owned(),
                    ..Item::new(shown, run)
                })
            })
            .collect()
    }

    pub fn execute(&self, run: &Run) {
        match run {
            Run::App(app) => {
                if !app.entry.terminal() {
                    desktop::launch(&app.entry);
                    return;
                }
                desktop::launch_in_terminal(
                    &app.entry,
                    &format!(
                        "{} -e '{}'",
                        self.config().app_terminal,
                        cliphist::escape(&app.entry.command().join(" "))
                    ),
                );
            }
            Run::AppAction(app, index) => {
                if let Some(action) = app.actions.get(*index) {
                    desktop::launch_action(&app.entry, action);
                }
            }
            Run::Shell(line) => desktop::shell(line),
            Run::Clipboard(text) => {
                if let Some(display) = gtk4::gdk::Display::default() {
                    display.clipboard().set_text(text);
                }
            }
            Run::CopyEntry(entry) => self.cliphist.copy(entry),
            Run::DeleteEntry(entry) => self.cliphist.delete_entry(entry),
            Run::Builtin(name, arguments) => self.builtin(name, arguments),
            Run::Script(path, arguments) => {
                let mut argv = vec![path.as_str()];
                if !arguments.is_empty() {
                    argv.extend(arguments.split(' '));
                }
                detach(&argv);
            }
        }
    }

    fn builtin(&self, name: &str, arguments: &str) {
        match name {
            "accentcolor" if arguments.is_empty() => {
                crate::theming::switchwall::detach(&["--noswitch", "--color"]);
            }
            "accentcolor" => {
                crate::theming::switchwall::detach(&["--noswitch", "--color", arguments])
            }
            "dark" => crate::theming::switchwall::detach(&["--mode", "dark", "--noswitch"]),
            "light" => crate::theming::switchwall::detach(&["--mode", "light", "--noswitch"]),
            "superpaste" => self.superpaste(arguments),
            "todo" => self.todo.add(arguments),
            "wallpaper" => crate::core::actions::run("wallpaperSelectorToggle"),
            "wipeclipboard" => self.cliphist.wipe(),
            _ => {}
        }
    }

    fn superpaste(&self, arguments: &str) {
        let trimmed = arguments.trim();
        let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
        let Ok(count) = digits.parse::<usize>() else {
            let prefix = &self.config().search_action;
            detach(&[
                "notify-send",
                "Superpaste",
                &format!(
                    "Usage: <tt>{prefix}superpaste NUM_OF_ENTRIES[i]</tt>\nSupply <tt>i</tt> when you want images\nExamples:\n<tt>{prefix}superpaste 4i</tt> for the last 4 images\n<tt>{prefix}superpaste 7</tt> for the last 7 entries"
                ),
                "-a",
                "Shell",
            ]);
            return;
        };
        let images = trimmed[digits.len()..].starts_with('i');
        self.cliphist.superpaste(count, images);
    }
}

const BUILTINS: [&str; 7] = [
    "accentcolor",
    "dark",
    "light",
    "superpaste",
    "todo",
    "wallpaper",
    "wipeclipboard",
];

fn scripts() -> Vec<(String, String)> {
    let folder = crate::core::paths::config().join("actions");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<(String, String)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.starts_with('.'))
        })
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let stem = match name.rfind('.') {
                Some(at) if at > 0 => &name[..at],
                _ => name,
            };
            Some((stem.to_owned(), path.to_string_lossy().into_owned()))
        })
        .collect();
    found.sort_by(|left, right| left.1.cmp(&right.1));
    found
}

fn load_emojis() -> Vec<String> {
    let path = glib::user_config_dir().join("hypr/hyprland/scripts/fuzzel-emoji.sh");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines = text.split('\n');
    if !lines.any(|line| line == EMOJI_DATA) {
        return Vec::new();
    }
    lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_owned())
        .collect()
}

async fn read(line: &[&str]) -> Option<String> {
    let process = gio::Subprocess::newv(
        &line.iter().map(std::ffi::OsStr::new).collect::<Vec<_>>(),
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    )
    .ok()?;
    let (stdout, _) = process.communicate_utf8_future(None).await.ok()?;
    stdout.map(Into::into)
}
