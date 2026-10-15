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
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";
const DUPLICATE_WINDOW_MICROS: u64 = 2_000_000;

#[derive(Clone, Default)]
pub struct Track {
    pub bus: String,
    pub pid: Option<u32>,
    pub identity: String,
    pub desktop_entry: String,
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub art: String,
    pub url: String,
    pub position: i64,
    pub length: i64,
    pub is_length_supported: bool,
    pub is_playing: bool,
    pub can_seek: bool,
    pub can_control: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

impl Track {
    pub fn can_toggle_playing(&self) -> bool {
        self.can_control
            && if self.is_playing {
                self.can_pause
            } else {
                self.can_play
            }
    }

    fn end_position(&self) -> Option<i64> {
        (self.can_seek && self.is_length_supported && self.length > 0).then_some(self.length)
    }

    fn key(&self) -> String {
        format!("{}|{}", self.url, self.title)
    }
}

#[derive(Clone)]
pub struct Mpris {
    pub players: Rc<RefCell<Vec<Track>>>,
    tracked_bus: Rc<RefCell<Option<String>>>,
    owners: Rc<RefCell<HashMap<String, String>>>,
    known_lengths: Rc<RefCell<HashMap<String, (String, i64)>>>,
    session: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
}

impl Mpris {
    pub fn new(session: Option<gio::DBusConnection>) -> Self {
        let mpris = Mpris {
            players: Rc::new(RefCell::new(Vec::new())),
            tracked_bus: Rc::new(RefCell::new(None)),
            owners: Rc::new(RefCell::new(HashMap::new())),
            known_lengths: Rc::new(RefCell::new(HashMap::new())),
            session,
            listeners: Rc::default(),
        };
        let Some(session) = mpris.session.clone() else {
            return mpris;
        };

        std::mem::forget(session.subscribe_to_signal(
            None,
            Some("org.freedesktop.DBus.Properties"),
            Some("PropertiesChanged"),
            Some(OBJECT),
            None,
            gio::DBusSignalFlags::NONE,
            {
                let mpris = mpris.clone();
                move |signal| {
                    let changed_properties = signal.parameters.child_value(1);
                    let has_playback_changed = changed_properties
                        .iter()
                        .any(|entry| entry.child_value(0).str() == Some("PlaybackStatus"));
                    if has_playback_changed
                        && let Some(bus) = mpris.owners.borrow().get(signal.sender_name).cloned()
                    {
                        mpris.tracked_bus.replace(Some(bus));
                    }
                    mpris.refresh();
                }
            },
        ));
        std::mem::forget(session.subscribe_to_signal(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            Some(MPRIS),
            gio::DBusSignalFlags::MATCH_ARG0_NAMESPACE,
            {
                let mpris = mpris.clone();
                move |_| mpris.refresh()
            },
        ));

        mpris.refresh();
        mpris
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn active_track(&self) -> Option<Track> {
        let players = self.players.borrow();
        let tracked_bus = self.tracked_bus.borrow();
        tracked_bus
            .as_ref()
            .and_then(|bus| players.iter().find(|track| &track.bus == bus))
            .or_else(|| players.first())
            .cloned()
    }

    pub fn track_length(&self, track: &Track) -> i64 {
        if track.is_length_supported {
            return track.length;
        }
        match self.known_lengths.borrow().get(&track.bus) {
            Some((key, length)) if *key == track.key() => *length,
            _ => 0,
        }
    }

    pub fn distinct_tracks(&self) -> Vec<Track> {
        let players = self.players.borrow();
        let mut tracks: Vec<Track> = Vec::new();
        let mut is_grouped = vec![false; players.len()];
        let lengths: Vec<i64> = players
            .iter()
            .map(|track| self.track_length(track))
            .collect();
        for (index, first) in players.iter().enumerate() {
            if is_grouped[index] {
                continue;
            }
            let mut group = vec![index];
            let first_length = lengths[index];
            for (other, second) in players.iter().enumerate().skip(index + 1) {
                if is_grouped[other] {
                    continue;
                }
                let second_length = lengths[other];
                let titles_match = !first.title.is_empty()
                    && !second.title.is_empty()
                    && (first.title.contains(&second.title) || second.title.contains(&first.title));
                let positions_match = first_length > 0
                    && second_length > 0
                    && first.position.abs_diff(second.position) <= DUPLICATE_WINDOW_MICROS
                    && first_length.abs_diff(second_length) <= DUPLICATE_WINDOW_MICROS;
                if titles_match && positions_match {
                    is_grouped[other] = true;
                    group.push(other);
                }
            }
            let representative = group
                .iter()
                .copied()
                .find(|index| !players[*index].art.is_empty())
                .unwrap_or(group[0]);
            tracks.push(players[representative].clone());
        }
        tracks
    }

    pub fn toggle_playing(&self, track: &Track) {
        if track.can_toggle_playing() {
            self.command(&track.bus, "PlayPause");
        }
    }

    pub fn next(&self, track: &Track) {
        if track.can_go_next {
            self.command(&track.bus, "Next");
        }
    }

    pub fn previous(&self, track: &Track) {
        if track.can_go_previous {
            self.command(&track.bus, "Previous");
        }
    }

    pub fn skip_or_end(&self, track: &Track) {
        if track.can_go_next {
            self.next(track);
            return;
        }
        if let Some(end) = track.end_position() {
            self.seek(track, end);
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
        let track_id = parse_track_id(&track.track_id);
        let cached_position = track.position;
        glib::spawn_future_local(async move {
            let current_position = if track_id.is_none() {
                read_position(&session, &bus)
                    .await
                    .unwrap_or(cached_position)
            } else {
                cached_position
            };
            let (method, arguments) = prepare_seek(track_id, position, current_position);
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
            let names = read_bus_names(&session).await;
            let mut tracks: Vec<Track> = Vec::new();
            let mut owners = HashMap::new();
            for bus in &names {
                let mut track = read_track(&session, bus).await;
                if let Some(owner) = read_owner(&session, bus).await {
                    track.pid = read_process_id(&session, &owner).await;
                    owners.insert(owner, bus.clone());
                }
                tracks.push(track);
            }

            let previous_buses: Vec<String> = mpris
                .players
                .borrow()
                .iter()
                .map(|track| track.bus.clone())
                .collect();
            tracks.sort_by_key(|track| {
                previous_buses
                    .iter()
                    .position(|bus| *bus == track.bus)
                    .unwrap_or(usize::MAX)
            });
            mpris.owners.replace(owners);
            mpris.adopt(&previous_buses, &tracks);
            mpris.forget_lengths(&tracks);
            for track in &tracks {
                mpris.remember_length(track);
            }
            mpris.players.replace(tracks);
            mpris.listeners.notify();
        });
    }

    fn adopt(&self, previous_buses: &[String], tracks: &[Track]) {
        for track in tracks
            .iter()
            .filter(|track| !previous_buses.contains(&track.bus))
        {
            if self.tracked_bus.borrow().is_none() || track.is_playing {
                self.tracked_bus.replace(Some(track.bus.clone()));
            }
        }
        let is_tracked_gone = self
            .tracked_bus
            .borrow()
            .as_ref()
            .is_some_and(|bus| !tracks.iter().any(|track| &track.bus == bus));
        if is_tracked_gone {
            let replacement = tracks
                .iter()
                .find(|track| track.is_playing)
                .or_else(|| tracks.first())
                .map(|track| track.bus.clone());
            self.tracked_bus.replace(replacement);
        }
    }

    fn forget_lengths(&self, tracks: &[Track]) {
        self.known_lengths
            .borrow_mut()
            .retain(|bus, _| tracks.iter().any(|track| track.bus == *bus));
    }

    fn remember_length(&self, track: &Track) {
        if !track.is_length_supported || track.length <= 0 {
            return;
        }
        self.known_lengths
            .borrow_mut()
            .insert(track.bus.clone(), (track.key(), track.length));
    }

    pub fn follow_position(&self) {
        if self.players.borrow().iter().any(|track| track.is_playing) {
            self.refresh();
        }
    }
}

async fn read_bus_names(session: &gio::DBusConnection) -> Vec<String> {
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

async fn read_owner(session: &gio::DBusConnection, bus: &str) -> Option<String> {
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

async fn read_process_id(session: &gio::DBusConnection, owner: &str) -> Option<u32> {
    let reply = session
        .call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            Some(&(owner,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
        .ok()?;
    reply.child_value(0).get::<u32>()
}

async fn read_position(session: &gio::DBusConnection, bus: &str) -> Option<i64> {
    dbus::property(session, bus, OBJECT, INTERFACE, "Position")
        .await
        .and_then(|value| value.get::<i64>())
}

fn length_of(value: &Variant) -> Option<i64> {
    let length = value.get::<i64>().or_else(|| {
        value
            .get::<u64>()
            .and_then(|length| i64::try_from(length).ok())
    })?;
    (0..i64::MAX).contains(&length).then_some(length)
}

async fn read_track(session: &gio::DBusConnection, bus: &str) -> Track {
    let metadata = dbus::property(session, bus, OBJECT, INTERFACE, "Metadata").await;
    let entry = |key: &str| metadata.as_ref().and_then(|map| lookup(map, key));
    let flag = async |name: &str| {
        dbus::property(session, bus, OBJECT, INTERFACE, name)
            .await
            .and_then(|value| value.get::<bool>())
            .unwrap_or(false)
    };
    let length = entry("mpris:length").and_then(|value| length_of(&value));
    Track {
        bus: bus.to_owned(),
        pid: None,
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
        position: read_position(session, bus).await.unwrap_or(0),
        length: length.unwrap_or(0),
        is_length_supported: length.is_some(),
        is_playing: dbus::string_property(session, bus, OBJECT, INTERFACE, "PlaybackStatus").await
            == Some("Playing".to_owned()),
        can_seek: flag("CanSeek").await,
        can_control: flag("CanControl").await,
        can_play: flag("CanPlay").await,
        can_pause: flag("CanPause").await,
        can_go_next: flag("CanGoNext").await,
        can_go_previous: flag("CanGoPrevious").await,
    }
}

fn parse_track_id(track_id: &str) -> Option<glib::variant::ObjectPath> {
    if track_id == NO_TRACK {
        return None;
    }
    glib::variant::ObjectPath::try_from(track_id.to_owned()).ok()
}

fn prepare_seek(
    track_id: Option<glib::variant::ObjectPath>,
    position: i64,
    current_position: i64,
) -> (&'static str, Variant) {
    match track_id {
        Some(id) => ("SetPosition", (id, position).to_variant()),
        None => (
            "Seek",
            (position.saturating_sub(current_position),).to_variant(),
        ),
    }
}

fn lookup(map: &Variant, key: &str) -> Option<Variant> {
    map.iter()
        .find(|entry| entry.child_value(0).str() == Some(key))?
        .child_value(1)
        .as_variant()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_endless_stream_or_a_negative_length_has_no_length() {
        assert_eq!(length_of(&90_000_000i64.to_variant()), Some(90_000_000));
        assert_eq!(length_of(&90_000_000u64.to_variant()), Some(90_000_000));
        for length in [i64::MAX, -1, i64::MIN] {
            assert_eq!(length_of(&length.to_variant()), None);
        }
        assert_eq!(length_of(&u64::MAX.to_variant()), None);
    }

    fn make_track(bus: &str, position: i64) -> Track {
        Track {
            bus: bus.to_owned(),
            title: "Song".to_owned(),
            position,
            length: 180_000_000,
            is_length_supported: true,
            ..Track::default()
        }
    }

    fn list_distinct_buses(players: Vec<Track>) -> Vec<String> {
        let mpris = Mpris::new(None);
        mpris.players.replace(players);
        mpris
            .distinct_tracks()
            .into_iter()
            .map(|track| track.bus)
            .collect()
    }

    #[test]
    fn players_showing_one_track_share_a_card() {
        let players = vec![
            make_track("first", 0),
            Track {
                art: "file:///cover.png".to_owned(),
                ..make_track("second", 1_000_000)
            },
        ];
        assert_eq!(list_distinct_buses(players), ["second"]);
    }

    #[test]
    fn streams_without_a_length_never_share_a_card() {
        let stream = |bus: &str| Track {
            length: 0,
            is_length_supported: false,
            ..make_track(bus, 0)
        };
        assert_eq!(
            list_distinct_buses(vec![stream("first"), stream("second")]),
            ["first", "second"]
        );
    }

    #[test]
    fn a_player_matching_two_others_joins_only_the_first_group() {
        let players = vec![
            Track {
                title: "Song A".to_owned(),
                ..make_track("first", 0)
            },
            Track {
                title: "Song B".to_owned(),
                ..make_track("second", 0)
            },
            Track {
                art: "file:///cover.png".to_owned(),
                ..make_track("third", 0)
            },
        ];
        assert_eq!(list_distinct_buses(players), ["third", "second"]);
    }

    #[test]
    fn positions_and_lengths_at_the_ends_of_the_range_neither_overflow_nor_match() {
        let mpris = Mpris::new(None);
        mpris.players.replace(vec![
            make_track("first", i64::MIN),
            make_track("second", i64::MAX),
            make_track("third", 0),
            Track {
                length: i64::MIN,
                ..make_track("fourth", 0)
            },
            Track {
                length: i64::MAX,
                ..make_track("fifth", 0)
            },
        ]);
        assert_eq!(mpris.distinct_tracks().len(), 5);
    }

    #[test]
    fn lengths_of_players_that_are_gone_are_forgotten() {
        let mpris = Mpris::new(None);
        let kept = make_track("kept", 0);
        for track in [&kept, &make_track("gone", 0)] {
            mpris.remember_length(track);
        }
        let without_length = Track {
            length: 0,
            is_length_supported: false,
            ..kept
        };
        mpris.forget_lengths(std::slice::from_ref(&without_length));
        let buses: Vec<String> = mpris.known_lengths.borrow().keys().cloned().collect();
        assert_eq!(buses, ["kept"]);
        assert_eq!(mpris.track_length(&without_length), 180_000_000);
    }

    #[test]
    fn only_a_track_with_a_length_can_be_skipped_to_its_end() {
        let track = Track {
            can_seek: true,
            ..make_track("first", 0)
        };
        assert_eq!(track.end_position(), Some(180_000_000));
        let unseekable = Track {
            can_seek: false,
            ..track.clone()
        };
        assert_eq!(unseekable.end_position(), None);
        let without_length = Track {
            is_length_supported: false,
            ..track.clone()
        };
        assert_eq!(without_length.end_position(), None);
        assert_eq!(Track { length: 0, ..track }.end_position(), None);
    }

    #[test]
    fn a_seek_past_the_range_of_an_offset_stops_at_its_end() {
        let (method, arguments) = prepare_seek(None, i64::MAX, i64::MIN);
        assert_eq!((method, arguments), ("Seek", (i64::MAX,).to_variant()));
    }

    #[test]
    fn only_a_track_id_naming_a_track_takes_set_position() {
        let pick_method = |track_id: &str| prepare_seek(parse_track_id(track_id), 0, 0).0;
        assert_eq!(
            pick_method("/org/mpris/MediaPlayer2/Track/1"),
            "SetPosition"
        );
        assert_eq!(
            pick_method("/org/mpris/MediaPlayer2/TrackList/NoTrack"),
            "Seek"
        );
        assert_eq!(pick_method(""), "Seek");
    }

    #[test]
    fn a_seek_moves_by_the_distance_from_the_current_position() {
        let (method, arguments) = prepare_seek(None, 30_000_000, 20_000_000);
        assert_eq!((method, arguments), ("Seek", (10_000_000i64,).to_variant()));
    }
}
