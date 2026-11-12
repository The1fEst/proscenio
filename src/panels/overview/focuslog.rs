use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::env;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::core::listeners::Subscription;
use crate::core::{logfile, paths};
use crate::panels::overview::launcher::Launcher;
use crate::platform::hyprland::hypr::{self, Events};
use crate::platform::wayland::grab::Grab;

const PREFIX: &str = "overview focus lost: ";
const ENABLE_VARIABLE: &str = "PROSCENIO_FOCUS_LOG";
const FILE_NAME: &str = "launcher-focus.log";
const VERSION: &str = env!("PROSCENIO_VERSION");
const SETTLE_MILLIS: u64 = 300;
const CONFIRM_MILLIS: u64 = 50;
const EVENT_CAPACITY: usize = 64;
const LEVELS: [&str; 4] = ["background", "bottom", "top", "overlay"];
const WATCHED_EVENTS: [&str; 18] = [
    "openlayer",
    "closelayer",
    "openwindow",
    "closewindow",
    "activewindowv2",
    "focusedmonv2",
    "movewindowv2",
    "workspacev2",
    "activespecialv2",
    "fullscreen",
    "urgent",
    "minimized",
    "changefloatingmode",
    "pin",
    "togglegroup",
    "monitoraddedv2",
    "monitorremovedv2",
    "configreloaded",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reason {
    NotActiveAfterOpen,
    BecameInactive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordedEvent {
    offset_millis: u64,
    name: String,
    data: String,
}

#[derive(Default)]
struct EventRing {
    events: VecDeque<RecordedEvent>,
}

impl EventRing {
    fn push(&mut self, event: RecordedEvent) {
        if self.events.len() == EVENT_CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Toplevel {
    name: String,
    is_layer: bool,
    is_visible: bool,
    is_active: bool,
    keyboard_mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActiveWindow {
    address: String,
    class: String,
    title: String,
    is_xwayland: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct LayerGroup {
    monitor: String,
    level: &'static str,
    namespaces: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Activity {
    Never,
    ThenLost {
        since_millis: u64,
        active_millis: u64,
    },
}

struct Report {
    reason: Reason,
    elapsed_millis: u64,
    cause: String,
    window_before_open: Option<ActiveWindow>,
    was_active_at_open: bool,
    activity: Activity,
    monitor: String,
    keyboard_mode: String,
    grab_state: &'static str,
    query_length: usize,
    toplevels: Vec<Toplevel>,
    active_window: Option<ActiveWindow>,
    layers: Vec<LayerGroup>,
    events: Vec<RecordedEvent>,
}

pub struct FocusLog {
    events: Events,
    window: gtk4::ApplicationWindow,
    grab: Option<Rc<Grab>>,
    launcher: Launcher,
    generation: Cell<u64>,
    opened_at: Cell<Option<Instant>>,
    first_active_at: Cell<Option<Instant>>,
    lost_at: Cell<Option<Instant>>,
    was_active_at_open: Cell<bool>,
    window_before_open: RefCell<Option<ActiveWindow>>,
    is_reported: Cell<bool>,
    ring: Rc<RefCell<EventRing>>,
    subscription: RefCell<Option<Subscription>>,
}

impl FocusLog {
    pub fn new(
        events: &Events,
        window: &gtk4::ApplicationWindow,
        grab: Option<Rc<Grab>>,
        launcher: &Launcher,
    ) -> Rc<Self> {
        let log = Rc::new(FocusLog {
            events: events.clone(),
            window: window.clone(),
            grab,
            launcher: launcher.clone(),
            generation: Cell::new(0),
            opened_at: Cell::new(None),
            first_active_at: Cell::new(None),
            lost_at: Cell::new(None),
            was_active_at_open: Cell::new(false),
            window_before_open: RefCell::new(None),
            is_reported: Cell::new(false),
            ring: Rc::default(),
            subscription: RefCell::new(None),
        });
        window.connect_is_active_notify({
            let log = Rc::downgrade(&log);
            move |window| {
                if let Some(log) = log.upgrade() {
                    log.note_activity(window.is_active());
                }
            }
        });
        log
    }

    pub fn start(self: &Rc<Self>) {
        if !is_enabled() {
            return;
        }
        self.generation.set(self.generation.get() + 1);
        let opened_at = Instant::now();
        let is_active = self.window.is_active();
        self.opened_at.set(Some(opened_at));
        self.first_active_at.set(is_active.then_some(opened_at));
        self.lost_at.set(None);
        self.was_active_at_open.set(is_active);
        self.is_reported.set(false);
        self.window_before_open.replace(
            hypr::json("activewindow")
                .as_ref()
                .and_then(parse_active_window),
        );
        self.ring.take();
        let ring = self.ring.clone();
        self.subscription
            .replace(Some(self.events.subscribe(move |name, data| {
                if WATCHED_EVENTS.contains(&name) {
                    ring.borrow_mut().push(RecordedEvent {
                        offset_millis: opened_at.elapsed().as_millis() as u64,
                        name: name.to_owned(),
                        data: data.to_owned(),
                    });
                }
            })));
        self.schedule(Reason::NotActiveAfterOpen, SETTLE_MILLIS);
    }

    pub fn stop(&self) {
        self.opened_at.set(None);
        self.subscription.take();
        self.window_before_open.take();
        self.ring.take();
    }

    fn note_activity(self: &Rc<Self>, is_active: bool) {
        if self.opened_at.get().is_none() {
            return;
        }
        if is_active {
            if self.first_active_at.get().is_none() {
                self.first_active_at.set(Some(Instant::now()));
            }
            self.lost_at.set(None);
            return;
        }
        if self.first_active_at.get().is_some() {
            self.lost_at.set(Some(Instant::now()));
        }
        self.schedule(Reason::BecameInactive, CONFIRM_MILLIS);
    }

    fn schedule(self: &Rc<Self>, reason: Reason, delay_millis: u64) {
        if self.opened_at.get().is_none() || self.is_reported.get() {
            return;
        }
        let generation = self.generation.get();
        let log = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(delay_millis), move || {
            if let Some(log) = log.upgrade()
                && log.generation.get() == generation
                && log.opened_at.get().is_some()
                && !log.is_reported.get()
                && log.window.is_visible()
                && !log.window.is_active()
            {
                log.report(reason);
            }
        });
    }

    fn report(&self, reason: Reason) {
        let Some(opened_at) = self.opened_at.get() else {
            return;
        };
        self.is_reported.set(true);
        let report = self.gather(reason, opened_at);
        logfile::append_report(&log_path(), &format_report(&report));
    }

    fn gather(&self, reason: Reason, opened_at: Instant) -> Report {
        let now = Instant::now();
        let own_namespace: String = LayerShell::namespace(&self.window)
            .map(Into::into)
            .unwrap_or_default();
        let toplevels = self.collect_toplevels();
        let window_before_open = self.window_before_open.borrow().clone();
        let active_window = hypr::json("activewindow")
            .as_ref()
            .and_then(parse_active_window);
        let layers = hypr::json("layers")
            .as_ref()
            .map(parse_layers)
            .unwrap_or_default();
        let events: Vec<RecordedEvent> = self.ring.borrow().events.iter().cloned().collect();
        let activity = match self.first_active_at.get() {
            None => Activity::Never,
            Some(first) => Activity::ThenLost {
                since_millis: first.duration_since(opened_at).as_millis() as u64,
                active_millis: self
                    .lost_at
                    .get()
                    .unwrap_or(now)
                    .duration_since(first)
                    .as_millis() as u64,
            },
        };
        Report {
            reason,
            elapsed_millis: now.duration_since(opened_at).as_millis() as u64,
            cause: pick_cause(
                reason,
                &own_namespace,
                &events,
                &toplevels,
                window_before_open.as_ref(),
                active_window.as_ref(),
            ),
            window_before_open,
            was_active_at_open: self.was_active_at_open.get(),
            activity,
            monitor: self
                .window
                .monitor()
                .and_then(|monitor| monitor.connector())
                .map(Into::into)
                .unwrap_or_default(),
            keyboard_mode: format!("{:?}", self.window.keyboard_mode()),
            grab_state: match &self.grab {
                Some(grab) if grab.is_held() => "held",
                Some(_) => "released",
                None => "unavailable",
            },
            query_length: self.launcher.query.borrow().chars().count(),
            toplevels,
            active_window,
            layers,
            events,
        }
    }

    fn collect_toplevels(&self) -> Vec<Toplevel> {
        let own: &gtk4::Window = self.window.upcast_ref();
        gtk4::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk4::Window>().ok())
            .filter(|window| window != own)
            .map(|window| {
                let is_layer = window.is_layer_window();
                let name = if is_layer {
                    LayerShell::namespace(&window).map(Into::into)
                } else {
                    GtkWindowExt::title(&window).map(Into::into)
                };
                Toplevel {
                    name: name.unwrap_or_else(|| window.type_().name().to_owned()),
                    is_layer,
                    is_visible: window.is_visible(),
                    is_active: window.is_active(),
                    keyboard_mode: is_layer.then(|| format!("{:?}", window.keyboard_mode())),
                }
            })
            .collect()
    }
}

fn log_path() -> PathBuf {
    paths::state().join(FILE_NAME)
}

fn is_enabled() -> bool {
    static IS_ENABLED: LazyLock<bool> =
        LazyLock::new(|| is_enabled_by(env::var(ENABLE_VARIABLE).ok().as_deref()));
    *IS_ENABLED
}

fn is_enabled_by(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

pub fn log_start() {
    if !is_enabled() {
        return;
    }
    logfile::append_marker(&log_path(), &format!("proscenio started {VERSION}"));
}

fn parse_active_window(value: &Value) -> Option<ActiveWindow> {
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Some(ActiveWindow {
        address: value.get("address")?.as_str()?.to_owned(),
        class: text("class"),
        title: text("title"),
        is_xwayland: value
            .get("xwayland")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_layers(value: &Value) -> Vec<LayerGroup> {
    let Some(monitors) = value.as_object() else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    for (monitor, entry) in monitors {
        for (index, level) in LEVELS.iter().enumerate() {
            let namespaces: Vec<String> = entry
                .pointer(&format!("/levels/{index}"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|layer| layer.get("namespace")?.as_str().map(str::to_owned))
                .collect();
            if !namespaces.is_empty() {
                groups.push(LayerGroup {
                    monitor: monitor.clone(),
                    level,
                    namespaces,
                });
            }
        }
    }
    groups
}

fn strip_hex_prefix(address: &str) -> &str {
    address.strip_prefix("0x").unwrap_or(address)
}

fn describe(window: &ActiveWindow) -> String {
    format!("{} / {}", window.class, window.title)
}

fn label_opened_window(data: &str) -> String {
    let mut parts = data.splitn(4, ',').skip(2);
    match (parts.next(), parts.next()) {
        (Some(class), Some(title)) => format!("{class} / {title}"),
        _ => data.to_owned(),
    }
}

fn label_window(address: &str, events: &[RecordedEvent], known: &[&ActiveWindow]) -> String {
    if let Some(window) = known
        .iter()
        .find(|window| strip_hex_prefix(&window.address) == strip_hex_prefix(address))
    {
        return describe(window);
    }
    events
        .iter()
        .find(|event| {
            event.name == "openwindow"
                && event.data.split(',').next().map(strip_hex_prefix)
                    == Some(strip_hex_prefix(address))
        })
        .map_or_else(
            || format!("address {address}"),
            |event| label_opened_window(&event.data),
        )
}

fn find_latest(
    events: &[RecordedEvent],
    to_label: impl Fn(&RecordedEvent) -> Option<String>,
) -> Option<(u64, String)> {
    events
        .iter()
        .rev()
        .find_map(|event| Some((event.offset_millis, to_label(event)?)))
}

fn pick_cause(
    reason: Reason,
    own_namespace: &str,
    events: &[RecordedEvent],
    toplevels: &[Toplevel],
    window_before_open: Option<&ActiveWindow>,
    active_after: Option<&ActiveWindow>,
) -> String {
    if let Some(toplevel) = toplevels.iter().find(|toplevel| toplevel.is_active) {
        return format!("another window of the shell is active: {}", toplevel.name);
    }
    let known: Vec<&ActiveWindow> = window_before_open.into_iter().chain(active_after).collect();
    let is_before_open_window = |address: &str| {
        window_before_open
            .is_some_and(|window| strip_hex_prefix(&window.address) == strip_hex_prefix(address))
    };
    let activated = find_latest(events, |event| match event.name.as_str() {
        "activewindowv2" if !event.data.is_empty() && !is_before_open_window(&event.data) => {
            Some(label_window(&event.data, events, &known))
        }
        "openwindow" => Some(label_opened_window(&event.data)),
        _ => None,
    });
    if let Some((offset, label)) = activated {
        return format!("Hyprland activated another window: {label} (+{offset} ms)");
    }
    let appeared = find_latest(events, |event| {
        (event.name == "openlayer" && event.data != own_namespace).then(|| event.data.clone())
    });
    if let Some((offset, label)) = appeared {
        return format!("a layer surface appeared: {label} (+{offset} ms)");
    }
    let closed = find_latest(events, |event| match event.name.as_str() {
        "closewindow" => Some(label_window(&event.data, events, &known)),
        "closelayer" if event.data != own_namespace => Some(event.data.clone()),
        _ => None,
    });
    if let Some((offset, label)) = closed {
        return format!("a window or layer closed: {label} (+{offset} ms)");
    }
    if let (Some(before), Some(after)) = (window_before_open, active_after)
        && strip_hex_prefix(&before.address) == strip_hex_prefix(&after.address)
    {
        return format!(
            "the keyboard stayed on {}: Hyprland did not give the launcher the focus, or gave it back at once (no window or layer event explains it)",
            describe(before)
        );
    }
    if reason == Reason::NotActiveAfterOpen {
        return "the window never became active after opening".to_owned();
    }
    "no window or layer change was seen: a popup with a grab or a keyboard-mode change of an existing layer is likely (Hyprland sends no event for these)".to_owned()
}

fn format_report(report: &Report) -> String {
    let headline = match report.reason {
        Reason::NotActiveAfterOpen => "not active after open",
        Reason::BecameInactive => "became inactive",
    };
    let mut lines = vec![
        format!(
            "{PREFIX}{headline} ({} ms since open)",
            report.elapsed_millis
        ),
        format!("{PREFIX}cause: {}", report.cause),
    ];
    lines.push(match &report.window_before_open {
        Some(window) => format!(
            "{PREFIX}keyboard was on: {} (address {}, xwayland {})",
            describe(window),
            window.address,
            if window.is_xwayland { "yes" } else { "no" }
        ),
        None => format!("{PREFIX}keyboard was on: no window"),
    });
    let active_at_open = if report.was_active_at_open {
        "yes"
    } else {
        "no"
    };
    lines.push(match report.activity {
        Activity::Never => format!(
            "{PREFIX}launcher: active at open {active_at_open}, never became active"
        ),
        Activity::ThenLost {
            since_millis,
            active_millis,
        } => format!(
            "{PREFIX}launcher: active at open {active_at_open}, was active for {active_millis} ms (from +{since_millis} ms), then lost it"
        ),
    });
    lines.push(format!(
        "{PREFIX}window: monitor {}, keyboard mode {}, focus grab {}, query length {}",
        report.monitor, report.keyboard_mode, report.grab_state, report.query_length
    ));
    for toplevel in &report.toplevels {
        let kind = if toplevel.is_layer { "layer" } else { "window" };
        let visibility = if toplevel.is_visible {
            "visible"
        } else {
            "hidden"
        };
        let activity = if toplevel.is_active {
            "active"
        } else {
            "inactive"
        };
        let mode = toplevel
            .keyboard_mode
            .as_ref()
            .map(|mode| format!(", keyboard mode {mode}"))
            .unwrap_or_default();
        lines.push(format!(
            "{PREFIX}toplevel {kind} \"{}\": {visibility}, {activity}{mode}",
            toplevel.name
        ));
    }
    lines.push(match &report.active_window {
        Some(window) => format!(
            "{PREFIX}hyprland active window (live): class \"{}\", title \"{}\", address {}, xwayland {}",
            window.class,
            window.title,
            window.address,
            if window.is_xwayland { "yes" } else { "no" }
        ),
        None => format!("{PREFIX}hyprland active window (live): none"),
    });
    if report.layers.is_empty() {
        lines.push(format!("{PREFIX}hyprland layers (live): none"));
    }
    for group in &report.layers {
        lines.push(format!(
            "{PREFIX}hyprland layers (live) {} {}: {}",
            group.monitor,
            group.level,
            group.namespaces.join(", ")
        ));
    }
    lines.push(format!(
        "{PREFIX}events since open: {}",
        report.events.len()
    ));
    for event in &report.events {
        lines.push(format!(
            "{PREFIX}event +{} ms {}>>{}",
            event.offset_millis, event.name, event.data
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN_NAMESPACE: &str = "proscenio:overview";
    const NO_CHANGE: &str = "no window or layer change was seen: a popup with a grab or a keyboard-mode change of an existing layer is likely (Hyprland sends no event for these)";

    fn event(offset_millis: u64, name: &str, data: &str) -> RecordedEvent {
        RecordedEvent {
            offset_millis,
            name: name.to_owned(),
            data: data.to_owned(),
        }
    }

    fn toplevel(name: &str, is_active: bool) -> Toplevel {
        Toplevel {
            name: name.to_owned(),
            is_layer: true,
            is_visible: true,
            is_active,
            keyboard_mode: Some("OnDemand".to_owned()),
        }
    }

    fn window(address: &str, class: &str, title: &str) -> ActiveWindow {
        ActiveWindow {
            address: address.to_owned(),
            class: class.to_owned(),
            title: title.to_owned(),
            is_xwayland: false,
        }
    }

    struct Case {
        name: &'static str,
        reason: Reason,
        events: Vec<RecordedEvent>,
        toplevels: Vec<Toplevel>,
        before: Option<ActiveWindow>,
        after: Option<ActiveWindow>,
        expected: String,
    }

    fn case(name: &'static str, expected: &str) -> Case {
        Case {
            name,
            reason: Reason::BecameInactive,
            events: Vec::new(),
            toplevels: Vec::new(),
            before: None,
            after: None,
            expected: expected.to_owned(),
        }
    }

    #[test]
    fn only_a_non_empty_value_other_than_zero_enables_the_log() {
        for (value, expected) in [
            (None, false),
            (Some(""), false),
            (Some("0"), false),
            (Some("1"), true),
            (Some("true"), true),
        ] {
            assert_eq!(is_enabled_by(value), expected, "{value:?}");
        }
    }

    #[test]
    fn the_ring_keeps_only_the_latest_events_in_order() {
        let mut ring = EventRing::default();
        for offset in 0..(EVENT_CAPACITY as u64 + 3) {
            ring.push(event(offset, "openlayer", "x"));
        }
        let offsets: Vec<u64> = ring
            .events
            .iter()
            .map(|event| event.offset_millis)
            .collect();
        assert_eq!(offsets.len(), EVENT_CAPACITY);
        assert_eq!(offsets[0], 3);
        assert_eq!(*offsets.last().unwrap(), EVENT_CAPACITY as u64 + 2);
    }

    #[test]
    fn the_cause_follows_the_priority_and_names_the_matching_event() {
        let terminal = || Some(window("0x1", "kitty", "fish"));
        let browser = || window("0xabc", "firefox", "Docs");
        let cases = vec![
            Case {
                toplevels: vec![
                    toplevel("proscenio:osd", false),
                    toplevel("proscenio:bar", true),
                ],
                events: vec![event(5, "openlayer", "x")],
                ..case(
                    "shell window",
                    "another window of the shell is active: proscenio:bar",
                )
            },
            Case {
                events: vec![
                    event(7, "openlayer", "x"),
                    event(120, "activewindowv2", "abc"),
                ],
                before: terminal(),
                after: Some(browser()),
                ..case(
                    "activated by address",
                    "Hyprland activated another window: firefox / Docs (+120 ms)",
                )
            },
            Case {
                events: vec![event(120, "activewindowv2", "0xdef")],
                ..case(
                    "activated without a known window",
                    "Hyprland activated another window: address 0xdef (+120 ms)",
                )
            },
            Case {
                reason: Reason::NotActiveAfterOpen,
                events: vec![event(30, "openwindow", "def,2,kitty,~/Git, work")],
                before: terminal(),
                after: terminal(),
                ..case(
                    "activated by a new window",
                    "Hyprland activated another window: kitty / ~/Git, work (+30 ms)",
                )
            },
            Case {
                events: vec![
                    event(30, "openwindow", "def,2,kitty,shell"),
                    event(40, "activewindowv2", "def"),
                ],
                ..case(
                    "activation names the opened window",
                    "Hyprland activated another window: kitty / shell (+40 ms)",
                )
            },
            Case {
                events: vec![
                    event(0, "openlayer", OWN_NAMESPACE),
                    event(80, "openlayer", "swaync"),
                    event(90, "closelayer", "other"),
                ],
                before: terminal(),
                after: terminal(),
                ..case(
                    "layer appeared",
                    "a layer surface appeared: swaync (+80 ms)",
                )
            },
            Case {
                events: vec![event(60, "closewindow", "1")],
                before: terminal(),
                ..case(
                    "window closed",
                    "a window or layer closed: kitty / fish (+60 ms)",
                )
            },
            Case {
                events: vec![event(60, "closelayer", "swaync")],
                ..case("layer closed", "a window or layer closed: swaync (+60 ms)")
            },
            Case {
                events: vec![
                    event(0, "openlayer", OWN_NAMESPACE),
                    event(900, "closelayer", OWN_NAMESPACE),
                ],
                before: terminal(),
                after: terminal(),
                ..case(
                    "only own layer events keep the keyboard on the terminal",
                    "the keyboard stayed on kitty / fish: Hyprland did not give the launcher the focus, or gave it back at once (no window or layer event explains it)",
                )
            },
            Case {
                reason: Reason::NotActiveAfterOpen,
                events: vec![
                    event(1, "activewindowv2", ""),
                    event(500, "activewindowv2", "1"),
                ],
                before: terminal(),
                after: terminal(),
                ..case(
                    "the terminal activated again is not another window",
                    "the keyboard stayed on kitty / fish: Hyprland did not give the launcher the focus, or gave it back at once (no window or layer event explains it)",
                )
            },
            Case {
                events: vec![event(10, "focusedmonv2", "DP-2,1")],
                before: terminal(),
                after: Some(browser()),
                toplevels: vec![toplevel("proscenio:bar", false)],
                ..case(
                    "another window holds the keyboard without an event",
                    NO_CHANGE,
                )
            },
            Case {
                events: vec![event(1, "activewindowv2", "")],
                ..case("a cleared active window is not an activation", NO_CHANGE)
            },
            Case {
                reason: Reason::NotActiveAfterOpen,
                ..case(
                    "nothing seen after opening",
                    "the window never became active after opening",
                )
            },
        ];
        for case in cases {
            assert_eq!(
                pick_cause(
                    case.reason,
                    OWN_NAMESPACE,
                    &case.events,
                    &case.toplevels,
                    case.before.as_ref(),
                    case.after.as_ref()
                ),
                case.expected,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn hyprland_replies_become_an_active_window_and_layer_groups() {
        let parsed = parse_active_window(&serde_json::json!({
            "address": "0xabc", "class": "firefox", "title": "Docs", "xwayland": true
        }));
        assert_eq!(
            parsed,
            Some(ActiveWindow {
                is_xwayland: true,
                ..window("0xabc", "firefox", "Docs")
            })
        );
        assert_eq!(parse_active_window(&serde_json::json!({})), None);

        let layers = parse_layers(&serde_json::json!({
            "DP-2": { "levels": {
                "0": [],
                "1": [],
                "2": [{ "namespace": "proscenio:bar" }, { "namespace": "proscenio:overview" }],
                "3": [{ "namespace": "swaync" }]
            } }
        }));
        assert_eq!(
            layers,
            vec![
                LayerGroup {
                    monitor: "DP-2".to_owned(),
                    level: "top",
                    namespaces: vec!["proscenio:bar".to_owned(), "proscenio:overview".to_owned()],
                },
                LayerGroup {
                    monitor: "DP-2".to_owned(),
                    level: "overlay",
                    namespaces: vec!["swaync".to_owned()],
                },
            ]
        );
    }

    fn report(reason: Reason, activity: Activity, events: Vec<RecordedEvent>) -> Report {
        Report {
            reason,
            elapsed_millis: 412,
            cause: "a layer surface appeared: swaync (+380 ms)".to_owned(),
            window_before_open: Some(window("0x1", "kitty", "fish")),
            was_active_at_open: false,
            activity,
            monitor: "DP-2".to_owned(),
            keyboard_mode: "OnDemand".to_owned(),
            grab_state: "held",
            query_length: 3,
            toplevels: vec![
                toplevel("proscenio:bar", false),
                Toplevel {
                    name: "Settings".to_owned(),
                    is_layer: false,
                    is_visible: false,
                    is_active: false,
                    keyboard_mode: None,
                },
            ],
            active_window: Some(window("0x1", "kitty", "fish")),
            layers: vec![LayerGroup {
                monitor: "DP-2".to_owned(),
                level: "top",
                namespaces: vec!["proscenio:bar".to_owned(), "swaync".to_owned()],
            }],
            events,
        }
    }

    #[test]
    fn every_line_of_the_report_carries_the_prefix() {
        let text = format_report(&report(
            Reason::BecameInactive,
            Activity::ThenLost {
                since_millis: 40,
                active_millis: 350,
            },
            vec![
                event(0, "openlayer", "proscenio:overview"),
                event(380, "openlayer", "swaync"),
            ],
        ));
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.iter().all(|line| line.starts_with(PREFIX)));
        assert_eq!(
            lines[..5],
            [
                "overview focus lost: became inactive (412 ms since open)",
                "overview focus lost: cause: a layer surface appeared: swaync (+380 ms)",
                "overview focus lost: keyboard was on: kitty / fish (address 0x1, xwayland no)",
                "overview focus lost: launcher: active at open no, was active for 350 ms (from +40 ms), then lost it",
                "overview focus lost: window: monitor DP-2, keyboard mode OnDemand, focus grab held, query length 3",
            ]
        );
        assert!(lines.contains(
            &"overview focus lost: toplevel layer \"proscenio:bar\": visible, inactive, keyboard mode OnDemand"
        ));
        assert!(
            lines.contains(&"overview focus lost: toplevel window \"Settings\": hidden, inactive")
        );
        assert!(lines.contains(&"overview focus lost: hyprland active window (live): class \"kitty\", title \"fish\", address 0x1, xwayland no"));
        assert!(lines.contains(
            &"overview focus lost: hyprland layers (live) DP-2 top: proscenio:bar, swaync"
        ));
        assert!(lines.contains(&"overview focus lost: events since open: 2"));
        assert_eq!(
            lines[lines.len() - 2..],
            [
                "overview focus lost: event +0 ms openlayer>>proscenio:overview",
                "overview focus lost: event +380 ms openlayer>>swaync",
            ]
        );
    }

    #[test]
    fn a_launcher_that_never_became_active_and_saw_no_events_says_so() {
        let text = format_report(&report(
            Reason::NotActiveAfterOpen,
            Activity::Never,
            Vec::new(),
        ));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "overview focus lost: not active after open (412 ms since open)"
        );
        assert_eq!(
            lines[3],
            "overview focus lost: launcher: active at open no, never became active"
        );
        assert_eq!(
            lines.last(),
            Some(&"overview focus lost: events since open: 0")
        );
    }
}
