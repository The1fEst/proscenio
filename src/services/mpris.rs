use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::dbus;

const MPRIS: &str = "org.mpris.MediaPlayer2";
const OBJECT: &str = "/org/mpris/MediaPlayer2";
const INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const DUPLICATE_WINDOW: i64 = 2_000_000;

#[derive(Clone, Default)]
pub struct Track {
    pub bus: String,
    pub identity: String,
    pub desktop_entry: String,
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub art: String,
    pub url: String,
    pub position: i64,
    pub length: i64,
    pub length_supported: bool,
    pub playing: bool,
    pub can_seek: bool,
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
}

impl Track {
    pub fn can_toggle_playing(&self) -> bool {
        self.can_control
            && if self.playing {
                self.can_pause
            } else {
                self.can_play
            }
    }

    fn key(&self) -> String {
        format!("{}|{}", self.url, self.title)
    }
}

#[derive(Clone)]
pub struct Mpris {
    pub players: Rc<RefCell<Vec<Track>>>,
    tracked: Rc<RefCell<Option<String>>>,
    owners: Rc<RefCell<HashMap<String, String>>>,
    known_lengths: Rc<RefCell<HashMap<String, (String, i64)>>>,
    session: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
}

impl Mpris {
    pub fn new(session: Option<gio::DBusConnection>) -> Self {
        let mpris = Mpris {
            players: Rc::new(RefCell::new(Vec::new())),
            tracked: Rc::new(RefCell::new(None)),
            owners: Rc::new(RefCell::new(HashMap::new())),
            known_lengths: Rc::new(RefCell::new(HashMap::new())),
            session,
            listeners: Rc::default(),
        };
        let Some(session) = mpris.session.clone() else {
            return mpris;
        };

        let again = mpris.clone();
        std::mem::forget(session.subscribe_to_signal(
            None,
            Some("org.freedesktop.DBus.Properties"),
            Some("PropertiesChanged"),
            Some(OBJECT),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let changed = signal.parameters.child_value(1);
                let playback = changed
                    .iter()
                    .any(|entry| entry.child_value(0).str() == Some("PlaybackStatus"));
                if playback
                    && let Some(bus) = again.owners.borrow().get(signal.sender_name).cloned()
                {
                    again.tracked.replace(Some(bus));
                }
                again.refresh();
            },
        ));
        let again = mpris.clone();
        std::mem::forget(session.subscribe_to_signal(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            Some(MPRIS),
            gio::DBusSignalFlags::MATCH_ARG0_NAMESPACE,
            move |_| again.refresh(),
        ));

        mpris.refresh();
        mpris
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn active(&self) -> Option<Track> {
        let players = self.players.borrow();
        let tracked = self.tracked.borrow();
        tracked
            .as_ref()
            .and_then(|bus| players.iter().find(|track| &track.bus == bus))
            .or_else(|| players.first())
            .cloned()
    }

    pub fn track_length(&self, track: &Track) -> i64 {
        if track.length_supported {
            return track.length;
        }
        match self.known_lengths.borrow().get(&track.bus) {
            Some((key, length)) if *key == track.key() => *length,
            _ => 0,
        }
    }

    pub fn meaningful(&self) -> Vec<Track> {
        let players = self.players.borrow();
        let mut kept: Vec<Track> = Vec::new();
        let mut used = vec![false; players.len()];
        for (index, first) in players.iter().enumerate() {
            if used[index] {
                continue;
            }
            let mut group = vec![index];
            for (other, second) in players.iter().enumerate().skip(index + 1) {
                let titles = !first.title.is_empty()
                    && !second.title.is_empty()
                    && (first.title.contains(&second.title) || second.title.contains(&first.title));
                let places = (first.position - second.position).abs() <= DUPLICATE_WINDOW
                    && (first.length - second.length).abs() <= DUPLICATE_WINDOW;
                if titles && places {
                    group.push(other);
                }
            }
            let chosen = group
                .iter()
                .copied()
                .find(|index| !players[*index].art.is_empty())
                .unwrap_or(group[0]);
            kept.push(players[chosen].clone());
            for index in group {
                used[index] = true;
            }
        }
        kept
    }

    pub fn toggle_playing(&self, track: &Track) {
        if track.can_toggle_playing() {
            self.command(&track.bus, "PlayPause");
        }
    }

    pub fn next(&self, track: &Track) {
        if track.can_next {
            self.command(&track.bus, "Next");
        }
    }

    pub fn previous(&self, track: &Track) {
        if track.can_previous {
            self.command(&track.bus, "Previous");
        }
    }

    pub fn skip_or_end(&self, track: &Track) {
        if track.can_next {
            self.next(track);
            return;
        }
        if track.can_seek && track.length_supported {
            self.seek(track, self.track_length(track));
        }
    }

    pub fn pause_all(&self) {
        for track in self.players.borrow().iter() {
            if track.can_pause {
                self.command(&track.bus, "Pause");
            }
        }
    }

    pub fn command(&self, bus: &str, method: &'static str) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let bus = bus.to_owned();
        glib::spawn_future_local(async move {
            let _ = session
                .call_future(
                    Some(&bus),
                    OBJECT,
                    INTERFACE,
                    method,
                    None,
                    None,
                    gio::DBusCallFlags::NONE,
                    2000,
                )
                .await;
        });
    }

    pub fn seek(&self, track: &Track, position: i64) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let bus = track.bus.clone();
        let (method, arguments) = match glib::variant::ObjectPath::try_from(track.track_id.clone())
        {
            Ok(id) => ("SetPosition", (id, position).to_variant()),
            Err(_) => ("Seek", (position - track.position,).to_variant()),
        };
        glib::spawn_future_local(async move {
            let _ = session
                .call_future(
                    Some(&bus),
                    OBJECT,
                    INTERFACE,
                    method,
                    Some(&arguments),
                    None,
                    gio::DBusCallFlags::NONE,
                    2000,
                )
                .await;
        });
    }

    pub fn refresh(&self) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let mpris = self.clone();
        glib::spawn_future_local(async move {
            let names = names(&session).await;
            let mut found: Vec<Track> = Vec::new();
            let mut owners = HashMap::new();
            for bus in &names {
                if let Some(owner) = owner(&session, bus).await {
                    owners.insert(owner, bus.clone());
                }
                found.push(read(&session, bus).await);
            }

            let previous: Vec<String> = mpris
                .players
                .borrow()
                .iter()
                .map(|track| track.bus.clone())
                .collect();
            found.sort_by_key(|track| {
                previous
                    .iter()
                    .position(|bus| *bus == track.bus)
                    .unwrap_or(usize::MAX)
            });
            mpris.owners.replace(owners);
            mpris.adopt(&previous, &found);
            for track in &found {
                mpris.remember_length(track);
            }
            mpris.players.replace(found);
            mpris.listeners.notify();
        });
    }

    fn adopt(&self, previous: &[String], found: &[Track]) {
        for track in found.iter().filter(|track| !previous.contains(&track.bus)) {
            if self.tracked.borrow().is_none() || track.playing {
                self.tracked.replace(Some(track.bus.clone()));
            }
        }
        let gone = self
            .tracked
            .borrow()
            .as_ref()
            .is_some_and(|bus| !found.iter().any(|track| &track.bus == bus));
        if gone {
            let replacement = found
                .iter()
                .find(|track| track.playing)
                .or_else(|| found.first())
                .map(|track| track.bus.clone());
            self.tracked.replace(replacement);
        }
    }

    fn remember_length(&self, track: &Track) {
        if !track.length_supported || track.length <= 0 {
            return;
        }
        self.known_lengths
            .borrow_mut()
            .insert(track.bus.clone(), (track.key(), track.length));
    }

    pub fn follow_position(&self) {
        if self.players.borrow().iter().any(|track| track.playing) {
            self.refresh();
        }
    }
}

async fn names(session: &gio::DBusConnection) -> Vec<String> {
    let Ok(reply) = session
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
            None,
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
    else {
        return Vec::new();
    };
    reply
        .child_value(0)
        .iter()
        .filter_map(|name| name.str().map(str::to_owned))
        .filter(|name| {
            name.strip_prefix(MPRIS)
                .is_some_and(|rest| rest.starts_with('.'))
        })
        .collect()
}

async fn owner(session: &gio::DBusConnection, bus: &str) -> Option<String> {
    let reply = session
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
            Some(&(bus,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
        .ok()?;
    reply.child_value(0).str().map(str::to_owned)
}

async fn read(session: &gio::DBusConnection, bus: &str) -> Track {
    let metadata = dbus::property(session, bus, OBJECT, INTERFACE, "Metadata").await;
    let entry = |key: &str| metadata.as_ref().and_then(|map| lookup(map, key));
    let flag = async |name: &str| {
        dbus::property(session, bus, OBJECT, INTERFACE, name)
            .await
            .and_then(|value| value.get::<bool>())
            .unwrap_or(false)
    };
    let length = entry("mpris:length").and_then(|value| {
        value
            .get::<i64>()
            .or_else(|| value.get::<u64>().map(|length| length as i64))
    });
    Track {
        bus: bus.to_owned(),
        identity: dbus::string_property(session, bus, OBJECT, MPRIS, "Identity")
            .await
            .unwrap_or_default(),
        desktop_entry: dbus::string_property(session, bus, OBJECT, MPRIS, "DesktopEntry")
            .await
            .unwrap_or_default(),
        track_id: entry("mpris:trackid")
            .and_then(|value| value.str().map(str::to_owned))
            .unwrap_or_default(),
        title: entry("xesam:title")
            .and_then(|value| value.str().map(str::to_owned))
            .unwrap_or_default(),
        artist: entry("xesam:artist")
            .and_then(|value| {
                value
                    .iter()
                    .filter_map(|name| name.str().map(str::to_owned))
                    .next()
            })
            .unwrap_or_default(),
        art: entry("mpris:artUrl")
            .and_then(|value| value.str().map(str::to_owned))
            .unwrap_or_default(),
        url: entry("xesam:url")
            .and_then(|value| value.str().map(str::to_owned))
            .unwrap_or_default(),
        position: dbus::property(session, bus, OBJECT, INTERFACE, "Position")
            .await
            .and_then(|value| value.get::<i64>())
            .unwrap_or(0),
        length: length.unwrap_or(0),
        length_supported: length.is_some(),
        playing: dbus::string_property(session, bus, OBJECT, INTERFACE, "PlaybackStatus").await
            == Some("Playing".to_owned()),
        can_seek: flag("CanSeek").await,
        can_control: flag("CanControl").await,
        can_play: flag("CanPlay").await,
        can_pause: flag("CanPause").await,
        can_next: flag("CanGoNext").await,
        can_previous: flag("CanGoPrevious").await,
    }
}

fn lookup(map: &Variant, key: &str) -> Option<Variant> {
    map.iter()
        .find(|entry| entry.child_value(0).str() == Some(key))?
        .child_value(1)
        .as_variant()
}
