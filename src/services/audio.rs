use gtk4::glib;
use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::introspect::Introspector;
use libpulse_binding::context::subscribe::InterestMaskSet;
use libpulse_binding::context::{Context, FlagSet, State};
use libpulse_binding::volume::{ChannelVolumes, Volume};
use libpulse_glib_binding::Mainloop;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::tr;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::{assets, config, process};
use crate::platform::notify::{self, Notification};

const HARD_MAX: f64 = 2.0;
const SETTLE: Duration = Duration::from_secs(3);
const RECONNECT: Duration = Duration::from_secs(1);

#[derive(Clone, Copy)]
pub struct Protection {
    pub enable: bool,
    pub max_allowed_increase: f64,
    pub max_allowed: f64,
}

pub struct Stream {
    pub index: u32,
    pub name: String,
    pub media: Option<String>,
    pub icon: String,
    pub node: String,
    pub volume: f64,
    pub muted: bool,
}

pub struct Device {
    pub name: String,
    pub description: String,
    pub nick: String,
}

#[derive(Clone)]
pub struct Audio {
    pub sink_muted: Rc<Cell<bool>>,
    pub source_muted: Rc<Cell<bool>>,
    pub sink_volume: Rc<Cell<f64>>,
    pub source_volume: Rc<Cell<f64>>,
    sink_name: Rc<RefCell<String>>,
    source_name: Rc<RefCell<String>>,
    listeners: Rc<Listeners>,
    watchers: Rc<Listeners>,
    sink_listeners: Rc<Listeners>,
    protection_listeners: Rc<Listeners<String>>,
    sink_seen: Rc<RefCell<String>>,
    last_volume: Rc<Cell<f64>>,
    settled: Rc<Cell<bool>>,
    reconnecting: Rc<Cell<bool>>,
    mainloop: Rc<Mainloop>,
    context: Rc<RefCell<Context>>,
}

impl Audio {
    pub fn new() -> Option<Self> {
        let mainloop = Mainloop::new(None)?;
        let context = open(&mainloop)?;

        let audio = Audio {
            sink_muted: Rc::new(Cell::new(false)),
            source_muted: Rc::new(Cell::new(false)),
            sink_volume: Rc::new(Cell::new(0.0)),
            source_volume: Rc::new(Cell::new(0.0)),
            sink_name: Rc::new(RefCell::new(String::new())),
            source_name: Rc::new(RefCell::new(String::new())),
            listeners: Rc::default(),
            watchers: Rc::default(),
            sink_listeners: Rc::default(),
            protection_listeners: Rc::default(),
            sink_seen: Rc::new(RefCell::new(String::new())),
            last_volume: Rc::new(Cell::new(0.0)),
            settled: Rc::new(Cell::new(false)),
            reconnecting: Rc::new(Cell::new(false)),
            mainloop: Rc::new(mainloop),
            context: Rc::new(RefCell::new(context)),
        };

        let settled = audio.settled.clone();
        glib::timeout_add_local_once(SETTLE, move || settled.set(true));
        audio.follow();
        Some(audio)
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn watch(&self, listener: impl Fn() + 'static) -> Subscription {
        self.watchers.add(listener)
    }

    pub fn on_sink_change(&self, listener: impl Fn() + 'static) -> Subscription {
        self.sink_listeners.add(listener)
    }

    pub fn on_protection(&self, listener: impl Fn(&str) + 'static) -> Subscription {
        self.protection_listeners
            .add_with(move |reason: &String| listener(reason))
    }

    fn follow(&self) {
        let audio = self.clone();
        self.context
            .borrow_mut()
            .set_state_callback(Some(Box::new(move || {
                let audio = audio.clone();
                glib::idle_add_local_once(move || audio.on_state());
            })));
    }

    fn on_state(&self) {
        let state = self.context.borrow().get_state();
        match state {
            State::Ready => self.listen(),
            State::Failed | State::Terminated => self.reconnect(),
            _ => {}
        }
    }

    fn listen(&self) {
        self.refresh();
        let listener = self.clone();
        let mut context = self.context.borrow_mut();
        context.set_subscribe_callback(Some(Box::new(move |_, _, _| {
            listener.refresh();
            listener.watchers.notify();
        })));
        context.subscribe(
            InterestMaskSet::SERVER
                | InterestMaskSet::SINK
                | InterestMaskSet::SOURCE
                | InterestMaskSet::SINK_INPUT
                | InterestMaskSet::SOURCE_OUTPUT,
            |_| {},
        );
    }

    fn reconnect(&self) {
        if self.reconnecting.replace(true) {
            return;
        }
        let audio = self.clone();
        glib::timeout_add_local_once(RECONNECT, move || {
            audio.reconnecting.set(false);
            match open(&audio.mainloop) {
                Some(context) => {
                    audio.context.replace(context);
                    audio.follow();
                }
                None => audio.reconnect(),
            }
        });
    }

    fn introspector(&self) -> Option<Introspector> {
        let context = self.context.borrow();
        matches!(context.get_state(), State::Ready).then(|| context.introspect())
    }

    fn refresh(&self) {
        let Some(introspect) = self.introspector() else {
            return;
        };
        let audio = self.clone();
        introspect.get_server_info(move |info| {
            let defaults_moved = info
                .default_sink_name
                .as_deref()
                .is_some_and(|name| *audio.sink_name.borrow() != name)
                || info
                    .default_source_name
                    .as_deref()
                    .is_some_and(|name| *audio.source_name.borrow() != name);
            let Some(introspect) = audio.introspector() else {
                return;
            };
            if let Some(name) = info.default_sink_name.as_deref() {
                audio.sink_name.replace(name.to_owned());
                let sink = audio.clone();
                let owned = name.to_owned();
                introspect.get_sink_info_by_name(name, move |result| {
                    if let ListResult::Item(info) = result {
                        sink.take_sink(&owned, info.mute, info.volume);
                    }
                });
            }
            if let Some(name) = info.default_source_name.as_deref() {
                audio.source_name.replace(name.to_owned());
                let source = audio.clone();
                introspect.get_source_info_by_name(name, move |result| {
                    if let ListResult::Item(info) = result {
                        let was_muted = source.source_muted.get();
                        source.take(
                            &source.source_muted,
                            info.mute,
                            &source.source_volume,
                            info.volume,
                        );
                        if info.mute != was_muted && source.settled.get() {
                            announce_microphone(info.mute);
                        }
                    }
                });
            }
            if defaults_moved {
                audio.listeners.notify();
                audio.watchers.notify();
            }
        });
    }

    pub fn set_sink_volume(&self, part: f64) {
        let name = self.sink_name.borrow().clone();
        if name.is_empty() {
            return;
        }
        let Some(mut introspect) = self.introspector() else {
            return;
        };
        introspect.set_sink_volume_by_name(&name, &channels(part), None);
    }

    pub fn toggle_sink_mute(&self) {
        let name = self.sink_name.borrow().clone();
        if name.is_empty() {
            return;
        }
        let Some(mut introspect) = self.introspector() else {
            return;
        };
        introspect.set_sink_mute_by_name(&name, !self.sink_muted.get(), None);
    }

    pub fn toggle_source_mute(&self) {
        let name = self.source_name.borrow().clone();
        if name.is_empty() {
            return;
        }
        let Some(mut introspect) = self.introspector() else {
            return;
        };
        introspect.set_source_mute_by_name(&name, !self.source_muted.get(), None);
    }

    pub fn streams(&self, sink: bool, handler: impl Fn(Vec<Stream>) + 'static) {
        let found = Rc::new(RefCell::new(Vec::new()));
        let handler = Rc::new(handler);
        let describe = |proplist: &libpulse_binding::proplist::Proplist,
                        name: Option<String>,
                        index: u32,
                        volume: &ChannelVolumes,
                        muted: bool| Stream {
            index,
            name: proplist
                .get_str("application.name")
                .or_else(|| proplist.get_str("node.description"))
                .or(name.clone())
                .unwrap_or_default(),
            media: proplist.get_str("media.name"),
            icon: proplist
                .get_str("application.icon-name")
                .unwrap_or_default(),
            node: proplist.get_str("node.name").or(name).unwrap_or_default(),
            volume: volume.avg().0 as f64 / Volume::NORMAL.0 as f64,
            muted,
        };
        let Some(introspect) = self.introspector() else {
            return;
        };
        if sink {
            introspect.get_sink_input_info_list(move |result| match result {
                ListResult::Item(info) => found.borrow_mut().push(describe(
                    &info.proplist,
                    info.name.as_ref().map(|name| name.to_string()),
                    info.index,
                    &info.volume,
                    info.mute,
                )),
                ListResult::End => handler(found.take()),
                ListResult::Error => {}
            });
        } else {
            introspect.get_source_output_info_list(move |result| match result {
                ListResult::Item(info) => found.borrow_mut().push(describe(
                    &info.proplist,
                    info.name.as_ref().map(|name| name.to_string()),
                    info.index,
                    &info.volume,
                    info.mute,
                )),
                ListResult::End => handler(found.take()),
                ListResult::Error => {}
            });
        }
    }

    pub fn set_stream_volume(&self, sink: bool, index: u32, part: f64) {
        let volume = channels(part);
        let Some(mut introspect) = self.introspector() else {
            return;
        };
        if sink {
            introspect.set_sink_input_volume(index, &volume, None);
        } else {
            introspect.set_source_output_volume(index, &volume, None);
        }
    }

    pub fn set_stream_mute(&self, sink: bool, index: u32, muted: bool) {
        let Some(mut introspect) = self.introspector() else {
            return;
        };
        if sink {
            introspect.set_sink_input_mute(index, muted, None);
        } else {
            introspect.set_source_output_mute(index, muted, None);
        }
    }

    pub fn devices(&self, sink: bool, handler: impl Fn(Vec<Device>, String) + 'static) {
        let Some(introspect) = self.introspector() else {
            return;
        };
        let found: Rc<RefCell<Vec<Device>>> = Rc::new(RefCell::new(Vec::new()));
        let current = if sink {
            self.sink_name.borrow().clone()
        } else {
            self.source_name.borrow().clone()
        };
        if sink {
            introspect.get_sink_info_list(move |result| match result {
                ListResult::Item(info) => found.borrow_mut().push(Device {
                    name: info.name.as_deref().unwrap_or_default().to_owned(),
                    description: info.description.as_deref().unwrap_or_default().to_owned(),
                    nick: info.proplist.get_str("node.nick").unwrap_or_default(),
                }),
                ListResult::End => handler(found.take(), current.clone()),
                ListResult::Error => {}
            });
        } else {
            introspect.get_source_info_list(move |result| match result {
                ListResult::Item(info) => {
                    if info.monitor_of_sink.is_some() {
                        return;
                    }
                    found.borrow_mut().push(Device {
                        name: info.name.as_deref().unwrap_or_default().to_owned(),
                        description: info.description.as_deref().unwrap_or_default().to_owned(),
                        nick: info.proplist.get_str("node.nick").unwrap_or_default(),
                    })
                }
                ListResult::End => handler(found.take(), current.clone()),
                ListResult::Error => {}
            });
        }
    }

    pub fn set_default(&self, sink: bool, name: &str) {
        let mut context = self.context.borrow_mut();
        if !matches!(context.get_state(), State::Ready) {
            return;
        }
        if sink {
            context.set_default_sink(name, |_| {});
        } else {
            context.set_default_source(name, |_| {});
        }
    }

    pub fn set_source_volume(&self, part: f64) {
        let name = self.source_name.borrow().clone();
        if name.is_empty() {
            return;
        }
        let Some(mut introspect) = self.introspector() else {
            return;
        };
        introspect.set_source_volume_by_name(&name, &channels(part), None);
    }

    fn take_sink(&self, name: &str, muted: bool, volume: ChannelVolumes) {
        let part = part(volume);
        let fresh = *self.sink_seen.borrow() != name;
        if fresh {
            self.sink_seen.replace(name.to_owned());
            self.last_volume.set(part);
        }
        let moved = (self.sink_volume.get() - part).abs() >= f64::EPSILON;
        let changed = !fresh && (moved || self.sink_muted.get() != muted);
        if changed && moved {
            self.guard(part);
        }
        self.take(&self.sink_muted, muted, &self.sink_volume, volume);
        if changed {
            self.sink_listeners.notify();
        }
    }

    fn guard(&self, part: f64) {
        let protection = crate::core::config::current().protection;
        if !protection.enable {
            return;
        }
        let last = self.last_volume.get();
        if part - last > protection.max_allowed_increase {
            self.set_sink_volume(last);
            self.protect(&tr("Illegal increment"));
            return;
        }
        if part > protection.max_allowed || part > HARD_MAX {
            let allowed = last.min(protection.max_allowed);
            self.set_sink_volume(allowed);
            self.last_volume.set(allowed);
            self.protect(&tr("Exceeded max allowed"));
            return;
        }
        self.last_volume.set(part);
    }

    fn protect(&self, reason: &str) {
        self.protection_listeners.notify_with(&reason.to_owned());
    }

    fn take(
        &self,
        flag: &Rc<Cell<bool>>,
        muted: bool,
        level: &Rc<Cell<f64>>,
        volume: ChannelVolumes,
    ) {
        let part = part(volume);
        if flag.get() == muted && (level.get() - part).abs() < f64::EPSILON {
            return;
        }
        flag.set(muted);
        level.set(part);
        self.listeners.notify();
        self.watchers.notify();
    }
}

fn open(mainloop: &Mainloop) -> Option<Context> {
    let mut context = Context::new(mainloop, "proscenio")?;
    context.connect(None, FlagSet::NOFLAGS, None).ok()?;
    Some(context)
}

fn announce_microphone(muted: bool) {
    let state = tr(if muted { "Muted" } else { "Unmuted" });
    let icon = assets::microphone_icon().unwrap_or_default();
    notify::send(&Notification {
        app: "Microphone",
        summary: &tr("Microphone"),
        body: &state,
        icon: &icon.to_string_lossy(),
        transient: true,
        ..Default::default()
    });
    if config::value("/sounds/microphone")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
    {
        let theme = config::value("/sounds/theme")
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "freedesktop".to_owned());
        play_system_sound(
            &theme,
            if muted {
                "device-removed"
            } else {
                "device-added"
            },
        );
    }
}

pub fn sound_themes() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("/usr/share/sounds") else {
        return Vec::new();
    };
    let mut themes: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("stereo").is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    themes.sort();
    themes.dedup();
    themes
}

#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub name: String,
    pub description: String,
    pub active: String,
    pub profiles: Vec<(String, String)>,
}

pub fn parse_cards(text: &str) -> Vec<Card> {
    let mut cards: Vec<Card> = Vec::new();
    let mut active = String::new();
    let mut known: Vec<String> = Vec::new();
    let mut in_profiles = false;
    let finish = |cards: &mut Vec<Card>, active: &str, known: &[String]| {
        if let Some(card) = cards.last_mut() {
            card.active = if known.iter().any(|profile| profile == active) {
                active.to_owned()
            } else {
                "off".to_owned()
            };
        }
    };
    for line in text.lines() {
        let depth = line
            .chars()
            .take_while(|character| *character == '\t')
            .count();
        let content = line.trim();
        if line.starts_with("Card #") {
            finish(&mut cards, &active, &known);
            active.clear();
            known.clear();
            in_profiles = false;
            cards.push(Card {
                name: String::new(),
                description: String::new(),
                active: String::new(),
                profiles: Vec::new(),
            });
            continue;
        }
        let Some(card) = cards.last_mut() else {
            continue;
        };
        if depth == 1 {
            in_profiles = content == "Profiles:";
            if let Some(name) = content.strip_prefix("Name: ") {
                card.name = name.to_owned();
            } else if let Some(profile) = content.strip_prefix("Active Profile: ") {
                active = profile.to_owned();
            }
        } else if depth == 2 && in_profiles {
            let Some((key, rest)) = content.split_once(": ") else {
                continue;
            };
            known.push(key.to_owned());
            let (label, details) = rest.rsplit_once(" (").unwrap_or((rest, ""));
            if details.contains("available: yes") {
                card.profiles.push((label.to_owned(), key.to_owned()));
            }
        } else if depth == 2
            && let Some(description) = content.strip_prefix("device.description = ")
        {
            card.description = description.trim_matches('"').to_owned();
        }
    }
    finish(&mut cards, &active, &known);
    for card in &mut cards {
        if card.description.is_empty() {
            card.description = card.name.clone();
        }
    }
    cards
}

pub fn cards(handler: impl FnOnce(Vec<Card>) + 'static) {
    let mut command = process::command(&["pactl", "list", "cards"]);
    command.env("LC_ALL", "C");
    glib::spawn_future_local(async move {
        if let Some(output) = process::capture_text(command).await {
            handler(parse_cards(&output));
        }
    });
}

pub fn set_card_profile(card: &str, profile: &str, then: impl FnOnce() + 'static) {
    let (card, profile) = (card.to_owned(), profile.to_owned());
    glib::spawn_future_local(async move {
        let command = ["pactl", "set-card-profile", card.as_str(), profile.as_str()];
        process::finish(process::command(&command)).await;
        then();
    });
}

pub fn play_system_sound(theme: &str, name: &str) {
    let found = ["oga", "ogg"]
        .map(|extension| format!("/usr/share/sounds/{theme}/stereo/{name}.{extension}"))
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file());
    if let Some(path) = found {
        process::detach(&["paplay", &path]);
    }
}

fn part(volume: ChannelVolumes) -> f64 {
    volume.avg().0 as f64 / Volume::NORMAL.0 as f64
}

fn channels(part: f64) -> ChannelVolumes {
    let mut volume = ChannelVolumes::default();
    volume.set(
        2,
        Volume((part.clamp(0.0, 1.0) * Volume::NORMAL.0 as f64).round() as u32),
    );
    volume
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_keep_their_available_profiles_in_pactl_order() {
        let text = "Card #44\n\tName: alsa_card.pci-0000_00_05.0\n\tProperties:\n\t\tdevice.description = \"Built-in Audio\"\n\tProfiles:\n\t\toff: Off (sinks: 0, sources: 0, priority: 0, available: yes)\n\t\toutput:analog-stereo+input:analog-stereo: Analog Stereo Duplex (sinks: 1, sources: 1, priority: 6565, available: yes)\n\t\toutput:hdmi: HDMI (sinks: 1, sources: 0, priority: 5900, available: no)\n\tActive Profile: output:analog-stereo+input:analog-stereo\n\tPorts:\n\t\tanalog-output: Line Out (type: Line, priority: 9000)\n";
        assert_eq!(
            parse_cards(text),
            [Card {
                name: "alsa_card.pci-0000_00_05.0".to_owned(),
                description: "Built-in Audio".to_owned(),
                active: "output:analog-stereo+input:analog-stereo".to_owned(),
                profiles: vec![
                    ("Off".to_owned(), "off".to_owned()),
                    (
                        "Analog Stereo Duplex".to_owned(),
                        "output:analog-stereo+input:analog-stereo".to_owned()
                    ),
                ],
            }]
        );
    }
}
