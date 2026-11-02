use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::config::{self, Config};
use crate::core::dmenu::{Reply, Request, Responder, Sessions};
use crate::core::frecency::{self, UsageHistory};
use crate::core::fuzzy::{self, SearchTarget};
use crate::core::i18n::{tr, trf};
use crate::core::levenshtein;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::dbus::notify::{self, Notification};
use crate::platform::system::desktop::{self, DesktopAction, DesktopEntry};
use crate::services::files::cliphist::{self, Cliphist};
use crate::services::network::net::Net;

const EMOJI_DATA: &str = "### DATA ###";
const FUZZY_BONUS: f64 = 10.0;
const LEVENSHTEIN_BONUS: f64 = 0.1;

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
    pub is_monospace: bool,
    pub name: String,
    pub raw_entry: String,
    pub icon: String,
    pub icon_type: IconType,
    pub verb: String,
    pub should_blur: bool,
    pub run: Run,
    pub actions: Vec<Item>,
}

impl Item {
    fn new(name: &str, run: Run) -> Self {
        Item {
            kind: String::new(),
            is_monospace: false,
            name: name.to_owned(),
            raw_entry: String::new(),
            icon: String::new(),
            icon_type: IconType::None,
            verb: String::new(),
            should_blur: false,
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
    Dmenu(u64, usize),
}

struct Emojis {
    lines: Vec<String>,
    names: Vec<SearchTarget>,
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
    math_answer: Rc<RefCell<String>>,
    math_timer: Rc<RefCell<Option<glib::SourceId>>>,
    math_generation: Rc<Cell<u64>>,
    apps: Rc<RefCell<Vec<Rc<App>>>>,
    app_names: Rc<RefCell<Vec<SearchTarget>>>,
    history: Rc<RefCell<UsageHistory>>,
    emojis: Rc<RefCell<Option<Rc<Emojis>>>>,
    emoji_generation: Rc<Cell<u64>>,
    is_emoji_loading: Rc<Cell<bool>>,
    dmenu: Rc<RefCell<Sessions>>,
    cliphist: Cliphist,
    net: Net,
    listeners: Rc<Listeners>,
}

impl Launcher {
    pub fn new(cliphist: &Cliphist, net: &Net) -> Self {
        let launcher = Launcher {
            query: Rc::new(RefCell::new(String::new())),
            math_answer: Rc::new(RefCell::new(String::new())),
            math_timer: Rc::new(RefCell::new(None)),
            math_generation: Rc::new(Cell::new(0)),
            apps: Rc::new(RefCell::new(Vec::new())),
            app_names: Rc::new(RefCell::new(Vec::new())),
            history: Rc::default(),
            emojis: Rc::new(RefCell::new(None)),
            emoji_generation: Rc::new(Cell::new(0)),
            is_emoji_loading: Rc::new(Cell::new(false)),
            dmenu: Rc::default(),
            cliphist: cliphist.clone(),
            net: net.clone(),
            listeners: Rc::new(Listeners::default()),
        };
        cliphist
            .subscribe({
                let launcher = launcher.clone();
                move || {
                    if launcher.dmenu_session() == 0 {
                        launcher.announce();
                    }
                }
            })
            .detach();
        launcher
    }

    fn config(&self) -> Rc<Config> {
        config::current()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn announce(&self) {
        self.listeners.notify();
    }

    pub fn begin_dmenu(&self, request: Request, responder: Responder) -> u64 {
        if let Some(source) = self.math_timer.borrow_mut().take() {
            source.remove();
        }
        self.math_generation.set(self.math_generation.get() + 1);
        let on_abandon = responder.on_abandon();
        let session = self.dmenu.borrow_mut().begin(request, responder);
        let launcher = self.clone();
        on_abandon.set(move || {
            launcher.finish_dmenu(session, Reply::Cancellation);
            launcher.announce();
        });
        session
    }

    pub fn dmenu_session(&self) -> u64 {
        self.dmenu.borrow().id()
    }

    pub fn finish_dmenu(&self, session: u64, reply: Reply) {
        self.dmenu.borrow_mut().finish(session, reply);
    }

    pub fn cancel_dmenu(&self) {
        if self.dmenu.borrow_mut().cancel() {
            self.announce();
        }
    }

    pub fn load(&self) {
        self.reload_apps();
        self.history.replace(UsageHistory::load());
        self.cliphist.load();
    }

    pub fn release(&self) {
        self.apps.replace(Vec::new());
        self.app_names.replace(Vec::new());
        self.history.replace(UsageHistory::default());
        self.emojis.replace(None);
        self.emoji_generation.set(self.emoji_generation.get() + 1);
        self.is_emoji_loading.set(false);
        self.math_answer.replace(String::new());
        self.cliphist.release();
    }

    fn emojis(&self) -> Option<Rc<Emojis>> {
        if let Some(emojis) = self.emojis.borrow().as_ref() {
            return Some(emojis.clone());
        }
        if self.is_emoji_loading.replace(true) {
            return None;
        }
        let generation = self.emoji_generation.get();
        let launcher = self.clone();
        glib::spawn_future_local(async move {
            let load_result = gio::spawn_blocking(|| {
                let lines = load_emojis();
                let names: Vec<SearchTarget> =
                    lines.iter().map(|line| fuzzy::prepare(line)).collect();
                (lines, names)
            })
            .await;
            if launcher.emoji_generation.get() != generation {
                return;
            }
            launcher.is_emoji_loading.set(false);
            let Ok((lines, names)) = load_result else {
                return;
            };
            launcher
                .emojis
                .replace(Some(Rc::new(Emojis { lines, names })));
            launcher.announce();
        });
        None
    }

    fn reload_apps(&self) {
        let apps: Vec<Rc<App>> = desktop::all()
            .into_iter()
            .filter(DesktopEntry::is_shown)
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
        let is_special = query.is_empty()
            || self.dmenu_session() != 0
            || query.starts_with(&self.config().search_clipboard)
            || query.starts_with(&self.config().search_emojis);
        if !is_special {
            self.restart_math_timer();
        }
        self.announce();
    }

    pub fn clean_one_prefix<'a>(&self, text: &'a str) -> &'a str {
        if self.dmenu_session() != 0 {
            return text;
        }
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
            if *launcher.math_answer.borrow() == answer {
                return;
            }
            launcher.math_answer.replace(answer.to_owned());
            launcher.announce();
        });
    }

    fn dmenu_results(&self, query: &str) -> Vec<Item> {
        let sessions = self.dmenu.borrow();
        let session = sessions.id();
        sessions
            .search(query, levenshtein::is_enabled())
            .into_iter()
            .filter_map(|index| Some(Item::new(sessions.item(index)?, Run::Dmenu(session, index))))
            .collect()
    }

    pub fn results(&self) -> Vec<Item> {
        let query = self.query.borrow().clone();
        if self.dmenu_session() != 0 {
            return self.dmenu_results(&query);
        }
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

        let answer = self.math_answer.borrow().clone();
        let math = Item {
            kind: tr("Math result"),
            is_monospace: true,
            icon: "calculate".to_owned(),
            icon_type: IconType::Material,
            verb: tr("Copy"),
            ..Item::new(&answer, Run::Clipboard(answer.clone()))
        };
        let command_name = query
            .strip_prefix(config.search_shell.as_str())
            .unwrap_or(&query)
            .replace("file://", "");
        let command = Item {
            kind: tr("Command"),
            is_monospace: true,
            icon: "terminal".to_owned(),
            icon_type: IconType::Material,
            verb: tr("Run"),
            ..Item::new(&command_name, Run::Shell(self.command_line(&query)))
        };

        let apps = self.app_results(
            query
                .strip_prefix(config.search_app.as_str())
                .unwrap_or(&query),
        );
        let actions = self.action_results(&query);

        let starts_with_number = query.starts_with(|letter: char| letter.is_ascii_digit());
        let has_math_prefix = query.starts_with(config.search_math.as_str());
        let has_shell_prefix = query.starts_with(config.search_shell.as_str());

        let mut results = Vec::new();
        if starts_with_number || has_math_prefix {
            results.push(math.clone());
        } else if has_shell_prefix {
            results.push(command.clone());
        }
        results.extend(apps);
        results.extend(actions);
        if config.search_default_actions {
            if !has_shell_prefix {
                results.push(command);
            }
            if !starts_with_number && !has_math_prefix {
                results.push(math);
            }
        }
        results
    }

    fn command_line(&self, query: &str) -> String {
        let config = self.config();
        let shell = config.search_shell.as_str();
        let command = query.replace("file://", "");
        let command = command.strip_prefix(shell).unwrap_or(&command);
        let command = command.strip_prefix(shell).unwrap_or(command);
        replace_sudo(command)
    }

    fn app_results(&self, search: &str) -> Vec<Item> {
        let apps = self.apps.borrow();
        let (matches, bonus) = if levenshtein::is_enabled() {
            let names = apps.iter().map(|app| app.name.as_str());
            (
                levenshtein::rank_with_scores(names, search, levenshtein::score),
                LEVENSHTEIN_BONUS,
            )
        } else {
            let names = self.app_names.borrow();
            let references: Vec<&SearchTarget> = names.iter().collect();
            (fuzzy::go_with_scores(search, &references), FUZZY_BONUS)
        };
        let history = self.history.borrow();
        let now_secs = frecency::now_secs();
        let indices = order_by_frecency(matches, bonus, |index| {
            apps.get(index)
                .map_or(0.0, |app| history.score(&app.entry.id, now_secs))
        });
        indices
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
                    kind: tr("App"),
                    icon: app.icon.clone(),
                    icon_type: IconType::System,
                    verb: tr("Open"),
                    actions,
                    ..Item::new(&app.name, Run::App(app.clone()))
                }
            })
            .collect()
    }

    fn clipboard_results(&self, search: &str) -> Vec<Item> {
        let entries = self.cliphist.fuzzy_query(search);
        let is_guarded = self.is_clipboard_guarded();
        let config = self.config();
        let is_unsafe_link = |entry: Option<&String>| {
            entry.is_some_and(|entry| {
                let lowercase = entry.to_lowercase();
                config
                    .safety_links
                    .iter()
                    .any(|keyword| lowercase.contains(keyword.as_str()))
            })
        };
        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let should_blur = is_guarded
                    && cliphist::is_image(entry)
                    && (is_unsafe_link(
                        index.checked_sub(1).and_then(|before| entries.get(before)),
                    ) || is_unsafe_link(entries.get(index + 1)));
                let entry_id = entry.split_whitespace().next().unwrap_or("");
                Item {
                    kind: format!("#{entry_id}"),
                    raw_entry: entry.clone(),
                    should_blur,
                    actions: vec![
                        Item {
                            icon: "content_copy".to_owned(),
                            icon_type: IconType::Material,
                            ..Item::new(&tr("Copy"), Run::CopyEntry(entry.clone()))
                        },
                        Item {
                            icon: "delete".to_owned(),
                            icon_type: IconType::Material,
                            ..Item::new(&tr("Delete"), Run::DeleteEntry(entry.clone()))
                        },
                    ],
                    ..Item::new(cliphist::clean(entry), Run::CopyEntry(entry.clone()))
                }
            })
            .collect()
    }

    fn is_clipboard_guarded(&self) -> bool {
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
        let Some(emojis) = self.emojis() else {
            return Vec::new();
        };
        fuzzy::rank_lines(
            &emojis.lines,
            &emojis.names,
            search,
            levenshtein::is_enabled(),
        )
        .into_iter()
        .filter_map(|index| emojis.lines.get(index))
        .map(|line| {
            let emoji = line.split_whitespace().next().unwrap_or("").to_owned();
            Item {
                kind: tr("Emoji"),
                raw_entry: line.clone(),
                icon: emoji.clone(),
                icon_type: IconType::Text,
                verb: tr("Copy"),
                ..Item::new(cliphist::without_first_word(line), Run::Clipboard(emoji))
            }
        })
        .collect()
    }

    fn action_results(&self, query: &str) -> Vec<Item> {
        let arguments = query.split(' ').skip(1).collect::<Vec<_>>().join(" ");
        let mut candidates: Vec<(String, Run)> = BUILTINS
            .iter()
            .map(|name| (name.to_string(), Run::Builtin(name, arguments.clone())))
            .collect();
        candidates.extend(
            scripts()
                .into_iter()
                .map(|(name, path)| (name, Run::Script(path, arguments.clone()))),
        );
        candidates
            .into_iter()
            .filter_map(|(name, run)| {
                let full_name = format!("{}{name}", self.config().search_action);
                if !full_name.starts_with(query) && !query.starts_with(&full_name) {
                    return None;
                }
                let display_name = if query.starts_with(&full_name) {
                    query
                } else {
                    &full_name
                };
                Some(Item {
                    kind: tr("Action"),
                    icon: "settings_suggest".to_owned(),
                    icon_type: IconType::Material,
                    verb: tr("Run"),
                    ..Item::new(display_name, run)
                })
            })
            .collect()
    }

    pub fn execute(&self, run: &Run) {
        match run {
            Run::App(app) => {
                record_launch(&app.entry.id);
                if !app.entry.runs_in_terminal() {
                    desktop::launch(&app.entry);
                    return;
                }
                desktop::launch_in_terminal(
                    &app.entry,
                    &terminal_line(&self.config().app_terminal, &app.entry.command()),
                );
            }
            Run::AppAction(app, index) => {
                if let Some(action) = app.actions.get(*index) {
                    record_launch(&app.entry.id);
                    desktop::launch_action(&app.entry, action);
                }
            }
            Run::Shell(line) => desktop::run_shell(line),
            Run::Clipboard(text) => {
                if let Some(display) = gtk4::gdk::Display::default() {
                    display.clipboard().set_text(text);
                }
            }
            Run::CopyEntry(entry) => self.cliphist.copy(entry),
            Run::DeleteEntry(entry) => self.cliphist.delete_entry(entry),
            Run::Builtin(name, arguments) => self.builtin(name, arguments),
            Run::Dmenu(session, index) => self.dmenu.borrow_mut().accept(*session, *index),
            Run::Script(path, arguments) => {
                let mut argv = vec![path.as_str()];
                if !arguments.is_empty() {
                    argv.extend(arguments.split(' '));
                }
                process::launch(&argv);
            }
        }
    }

    fn builtin(&self, name: &str, arguments: &str) {
        match name {
            "accentcolor" if arguments.is_empty() => {
                crate::theming::switchwallqueue::request(&["--noswitch", "--color"]);
            }
            "accentcolor" => {
                crate::theming::switchwallqueue::request(&["--noswitch", "--color", arguments])
            }
            "dark" => crate::theming::switchwallqueue::request(&["--mode", "dark", "--noswitch"]),
            "light" => crate::theming::switchwallqueue::request(&["--mode", "light", "--noswitch"]),
            "notepad" => crate::core::actions::run("sidebarLeftToggle"),
            "superpaste" => self.superpaste(arguments),
            "wallpaper" => crate::core::actions::run("wallpaperSelectorToggle"),
            "wipeclipboard" => self.cliphist.wipe(),
            _ => {}
        }
    }

    fn superpaste(&self, arguments: &str) {
        let argument_text = arguments.trim();
        let digits: String = argument_text
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let Ok(count) = digits.parse::<usize>() else {
            let prefix = &self.config().search_action;
            notify::send(&Notification {
                app: "Shell",
                summary: &tr("Superpaste"),
                body: &trf(
                    "Usage: <tt>%1superpaste NUM_OF_ENTRIES[i]</tt>\nSupply <tt>i</tt> when you want images\nExamples:\n<tt>%1superpaste 4i</tt> for the last 4 images\n<tt>%1superpaste 7</tt> for the last 7 entries",
                    &[prefix],
                ),
                ..Default::default()
            });
            return;
        };
        let wants_images = argument_text[digits.len()..].starts_with('i');
        self.cliphist.superpaste(count, wants_images);
    }
}

const BUILTINS: [&str; 7] = [
    "accentcolor",
    "dark",
    "light",
    "notepad",
    "superpaste",
    "wallpaper",
    "wipeclipboard",
];

fn scripts() -> Vec<(String, String)> {
    let folder = crate::core::paths::config().join("actions");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut script_entries: Vec<(String, String)> = entries
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
    script_entries.sort_by(|left, right| left.1.cmp(&right.1));
    script_entries
}

fn record_launch(id: &str) {
    let mut history = UsageHistory::load();
    history.record(id, frecency::now_secs());
    history.save();
}

fn order_by_frecency(
    matches: Vec<(usize, f64)>,
    bonus: f64,
    look_up_frecency: impl Fn(usize) -> f64,
) -> Vec<usize> {
    let mut totals: Vec<(usize, f64)> = matches
        .into_iter()
        .map(|(index, score)| (index, score + bonus * look_up_frecency(index)))
        .collect();
    totals.sort_by(|first, second| second.1.total_cmp(&first.1));
    totals.into_iter().map(|(index, _)| index).collect()
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
    process::capture_text(process::command(line)).await
}

fn terminal_line(terminal: &str, command: &[String]) -> String {
    let arguments: Vec<String> = command
        .iter()
        .map(|argument| format!("'{}'", cliphist::escape(argument)))
        .collect();
    format!("{terminal} -e {}", arguments.join(" "))
}

fn replace_sudo(command: &str) -> String {
    match command.strip_prefix("sudo") {
        Some(rest) if rest.starts_with(char::is_whitespace) => format!("pkexec{rest}"),
        _ => command.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sudo_command_asks_for_the_password_through_polkit() {
        assert_eq!(replace_sudo("sudo pacman -Syu"), "pkexec pacman -Syu");
        assert_eq!(
            replace_sudo("sudo  systemctl restart sddm"),
            "pkexec  systemctl restart sddm"
        );
        assert_eq!(replace_sudo("sudoku"), "sudoku");
        assert_eq!(replace_sudo("echo sudo"), "echo sudo");
    }

    const NOW_SECS: u64 = 1_800_000_000;
    const DAY_SECS: u64 = 86_400;

    fn rank_apps(names: &[&str], query: &str, history: &UsageHistory) -> Vec<usize> {
        let targets: Vec<SearchTarget> = names
            .iter()
            .map(|name| fuzzy::prepare(&format!("{name} ")))
            .collect();
        let references: Vec<&SearchTarget> = targets.iter().collect();
        order_by_frecency(
            fuzzy::go_with_scores(query, &references),
            FUZZY_BONUS,
            |index| history.score(names[index], NOW_SECS),
        )
    }

    fn build_history(entries: &[(&str, u32, u64)]) -> UsageHistory {
        let mut history = UsageHistory::default();
        for (id, count, age_days) in entries {
            for _ in 0..*count {
                history.record(id, NOW_SECS - age_days * DAY_SECS);
            }
        }
        history
    }

    #[test]
    fn the_frequently_used_app_wins_between_two_alike_matches() {
        let names = ["Firefox", "Firewall"];
        assert_eq!(rank_apps(&names, "fire", &UsageHistory::default()), [0, 1]);
        let history = build_history(&[("Firewall", 8, 1)]);
        assert_eq!(rank_apps(&names, "fire", &history), [1, 0]);
    }

    #[test]
    fn a_recent_launch_beats_an_equally_frequent_old_one() {
        let names = ["Firefox", "Firewall"];
        let history = build_history(&[("Firefox", 6, 120), ("Firewall", 6, 2)]);
        assert_eq!(rank_apps(&names, "fire", &history), [1, 0]);
    }

    #[test]
    fn a_much_better_match_beats_a_frequently_used_weak_one() {
        let names = ["File Roller", "Fish Shell", "Firefox"];
        let history = build_history(&[("File Roller", 30, 0)]);
        assert_eq!(rank_apps(&names, "fir", &history)[0], 2);
        assert_eq!(rank_apps(&names, "fir", &UsageHistory::default())[0], 2);
    }

    #[test]
    fn without_a_search_the_apps_follow_their_frecency() {
        let names = ["Alpha", "Beta", "Gamma"];
        let history = build_history(&[("Gamma", 5, 1), ("Beta", 2, 1)]);
        assert_eq!(rank_apps(&names, "", &history), [2, 1, 0]);
    }

    #[test]
    fn a_terminal_app_keeps_each_of_its_arguments_whole() {
        let command = ["htop", "--title=System Monitor", "it's"].map(str::to_owned);
        assert_eq!(
            terminal_line("kitty -1", &command),
            "kitty -1 -e 'htop' '--title=System Monitor' 'it'\\''s'"
        );
    }
}
