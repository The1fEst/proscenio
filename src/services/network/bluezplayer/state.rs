use gtk4::glib::{self, Variant, VariantTy};
use gtk4::prelude::*;
use std::hash::{DefaultHasher, Hash, Hasher};

use super::PATH;
use crate::core::shell;
use crate::services::media::mpris::{NO_TRACK, Track};

pub(super) type Properties = Vec<(&'static str, Variant)>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Action {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
}

impl Action {
    fn parse(method: &str) -> Option<Self> {
        match method {
            "Play" => Some(Action::Play),
            "Pause" => Some(Action::Pause),
            "PlayPause" => Some(Action::PlayPause),
            "Stop" => Some(Action::Stop),
            "Next" => Some(Action::Next),
            "Previous" => Some(Action::Previous),
            _ => None,
        }
    }

    fn is_supported(self, track: &Track) -> bool {
        track.can_control
            && match self {
                Action::Play => track.can_play,
                Action::Pause => track.can_pause,
                Action::PlayPause => track.can_toggle_playing(),
                Action::Stop => true,
                Action::Next => track.can_skip(),
                Action::Previous => track.can_go_previous,
            }
    }
}

pub(super) fn route(method: &str, track: &Track) -> Option<Action> {
    Action::parse(method).filter(|action| action.is_supported(track))
}

pub(super) fn list_properties(track: Option<&Track>, length: i64) -> Properties {
    let is_supported = |action: Action| track.is_some_and(|track| action.is_supported(track));
    let status = match track {
        Some(track) if track.is_playing => "Playing",
        Some(track) if !track.is_stopped => "Paused",
        _ => "Stopped",
    };
    let position = track
        .filter(|track| has_details(track, length))
        .map_or(0, |track| track.position.max(0));
    vec![
        ("PlaybackStatus", status.to_variant()),
        ("Metadata", build_metadata(track, length)),
        ("Identity", shell::name().to_variant()),
        (
            "CanControl",
            track.is_some_and(|track| track.can_control).to_variant(),
        ),
        ("CanPlay", is_supported(Action::Play).to_variant()),
        ("CanPause", is_supported(Action::Pause).to_variant()),
        ("CanGoNext", is_supported(Action::Next).to_variant()),
        ("CanGoPrevious", is_supported(Action::Previous).to_variant()),
        ("Position", position.to_variant()),
    ]
}

pub(super) fn list_changes(
    baseline: &[(&'static str, Variant)],
    properties: &[(&'static str, Variant)],
) -> Properties {
    let mut changes: Properties = properties
        .iter()
        .filter(|property| property.0 != "Position" && !baseline.contains(property))
        .cloned()
        .collect();
    if !changes.is_empty()
        && let Some(position) = properties.iter().find(|(name, _)| *name == "Position")
    {
        changes.push(position.clone());
    }
    changes
}

fn has_details(track: &Track, length: i64) -> bool {
    !track.title.is_empty() || !track.artist.is_empty() || !track.album.is_empty() || length > 0
}

fn build_metadata(track: Option<&Track>, length: i64) -> Variant {
    let track = track.filter(|track| has_details(track, length));
    let track_path = track.map_or_else(|| NO_TRACK.to_owned(), format_track_path);
    let mut entries: Properties = glib::variant::ObjectPath::try_from(track_path)
        .map(|path| vec![("mpris:trackid", path.to_variant())])
        .unwrap_or_default();
    if let Some(track) = track {
        for (key, text) in [("xesam:title", &track.title), ("xesam:album", &track.album)] {
            if !text.is_empty() {
                entries.push((key, text.to_variant()));
            }
        }
        if !track.artist.is_empty() {
            entries.push(("xesam:artist", vec![track.artist.clone()].to_variant()));
        }
        if length > 0 {
            entries.push(("mpris:length", length.to_variant()));
        }
    }
    build_dict(&entries)
}

fn format_track_path(track: &Track) -> String {
    let mut hasher = DefaultHasher::new();
    (
        &track.track_id,
        &track.url,
        &track.title,
        &track.artist,
        &track.album,
    )
        .hash(&mut hasher);
    format!("{PATH}/Track/{:016x}", hasher.finish())
}

pub(super) fn build_dict(entries: &[(&str, Variant)]) -> Variant {
    Variant::array_from_iter_with_type(
        VariantTy::VARDICT.element(),
        entries.iter().map(|(key, value)| {
            Variant::from_dict_entry(&key.to_variant(), &Variant::from_variant(value))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_playing_track() -> Track {
        Track {
            bus: "org.mpris.MediaPlayer2.player".to_owned(),
            title: "Song".to_owned(),
            artist: "Singer".to_owned(),
            album: "Record".to_owned(),
            position: 30_000_000,
            is_playing: true,
            can_control: true,
            can_play: true,
            can_pause: true,
            can_go_next: true,
            can_go_previous: true,
            ..Track::default()
        }
    }

    fn find_property(properties: &Properties, name: &str) -> Variant {
        properties
            .iter()
            .find(|(property_name, _)| *property_name == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("{name} is missing"))
    }

    fn find_metadata_entry(properties: &Properties, key: &str) -> Option<Variant> {
        glib::VariantDict::new(Some(&find_property(properties, "Metadata"))).lookup_value(key, None)
    }

    fn read_track_id(properties: &Properties) -> String {
        find_metadata_entry(properties, "mpris:trackid")
            .and_then(|value| value.get::<glib::variant::ObjectPath>())
            .map(|path| path.as_str().to_owned())
            .unwrap_or_default()
    }

    #[test]
    fn without_a_player_bluez_sees_a_stopped_player_without_a_track() {
        let no_track = glib::variant::ObjectPath::try_from(NO_TRACK.to_owned()).unwrap();
        let stopped_properties: Properties = vec![
            ("PlaybackStatus", "Stopped".to_variant()),
            (
                "Metadata",
                build_dict(&[("mpris:trackid", no_track.to_variant())]),
            ),
            ("Identity", shell::name().to_variant()),
            ("CanControl", false.to_variant()),
            ("CanPlay", false.to_variant()),
            ("CanPause", false.to_variant()),
            ("CanGoNext", false.to_variant()),
            ("CanGoPrevious", false.to_variant()),
            ("Position", 0i64.to_variant()),
        ];
        assert_eq!(list_properties(None, 0), stopped_properties);
    }

    #[test]
    fn the_position_follows_the_metadata_that_bluez_resets_it_on() {
        let track = build_playing_track();
        let find_index = |properties: &Properties, name: &str| {
            properties
                .iter()
                .position(|(property_name, _)| *property_name == name)
        };
        let next_track = Track {
            title: "Other song".to_owned(),
            ..track.clone()
        };
        let properties = list_properties(Some(&track), 0);
        let changes = list_changes(&properties, &list_properties(Some(&next_track), 0));
        for properties in [properties, changes] {
            assert!(find_index(&properties, "Metadata").is_some());
            assert!(find_index(&properties, "Position") > find_index(&properties, "Metadata"));
        }
    }

    #[test]
    fn a_playing_track_shows_its_status_metadata_and_buttons() {
        let properties = list_properties(Some(&build_playing_track()), 180_000_000);
        assert_eq!(
            find_property(&properties, "PlaybackStatus"),
            "Playing".to_variant()
        );
        assert_eq!(
            find_property(&properties, "Position"),
            30_000_000i64.to_variant()
        );
        let find_entry = |key: &str| find_metadata_entry(&properties, key);
        assert_eq!(find_entry("xesam:title"), Some("Song".to_variant()));
        assert_eq!(find_entry("xesam:album"), Some("Record".to_variant()));
        assert_eq!(
            find_entry("xesam:artist"),
            Some(vec!["Singer"].to_variant())
        );
        assert_eq!(
            find_entry("mpris:length"),
            Some(180_000_000i64.to_variant())
        );
        assert!(read_track_id(&properties).starts_with(&format!("{PATH}/Track/")));
        let paused_track = Track {
            is_playing: false,
            ..build_playing_track()
        };
        let stopped_track = Track {
            is_stopped: true,
            ..paused_track.clone()
        };
        for (track, status) in [(paused_track, "Paused"), (stopped_track, "Stopped")] {
            assert_eq!(
                find_property(&list_properties(Some(&track), 0), "PlaybackStatus"),
                status.to_variant()
            );
        }
    }

    #[test]
    fn the_buttons_follow_what_the_player_allows() {
        let uncontrolled_track = Track {
            can_control: false,
            ..build_playing_track()
        };
        let cases = [(build_playing_track(), true), (uncontrolled_track, false)];
        for (track, is_allowed) in cases {
            let properties = list_properties(Some(&track), 0);
            for name in [
                "CanControl",
                "CanPlay",
                "CanPause",
                "CanGoNext",
                "CanGoPrevious",
            ] {
                let value = find_property(&properties, name);
                assert_eq!(value, is_allowed.to_variant(), "{name}");
            }
        }
    }

    #[test]
    fn metadata_leaves_out_what_the_track_lacks() {
        let stream = Track {
            artist: String::new(),
            album: String::new(),
            ..build_playing_track()
        };
        let properties = list_properties(Some(&stream), 0);
        assert_eq!(
            find_metadata_entry(&properties, "xesam:title"),
            Some("Song".to_variant())
        );
        for key in ["xesam:artist", "xesam:album", "mpris:length"] {
            assert_eq!(find_metadata_entry(&properties, key), None, "{key}");
        }
        let untitled_track = Track {
            title: String::new(),
            ..stream
        };
        let untitled_properties = list_properties(Some(&untitled_track), 0);
        assert_eq!(read_track_id(&untitled_properties), NO_TRACK);
        assert_eq!(
            find_metadata_entry(&untitled_properties, "xesam:title"),
            None
        );
        let read_position =
            |track: &Track| find_property(&list_properties(Some(track), 0), "Position");
        assert_eq!(read_position(&untitled_track), 0i64.to_variant());
        assert_eq!(
            read_position(&Track {
                position: -5,
                ..build_playing_track()
            }),
            0i64.to_variant()
        );
    }

    #[test]
    fn the_track_id_follows_the_track() {
        let track = build_playing_track();
        let read_id = |track: &Track| read_track_id(&list_properties(Some(track), 180_000_000));
        let paused_track = Track {
            position: 0,
            is_playing: false,
            ..track.clone()
        };
        assert_eq!(read_id(&track), read_id(&paused_track));
        assert_eq!(
            read_id(&track),
            read_track_id(&list_properties(Some(&track), 0))
        );
        let next_track = Track {
            title: "Other song".to_owned(),
            ..track.clone()
        };
        assert_ne!(read_id(&track), read_id(&next_track));
        let same_tags_track = Track {
            track_id: "/org/mpris/MediaPlayer2/Track/2".to_owned(),
            ..track.clone()
        };
        assert_ne!(read_id(&track), read_id(&same_tags_track));
    }

    #[test]
    fn a_change_is_sent_with_the_position_and_the_position_alone_is_not() {
        let track = build_playing_track();
        let baseline = list_properties(Some(&track), 180_000_000);
        let later_track = Track {
            position: 33_000_000,
            ..track.clone()
        };
        assert_eq!(
            list_changes(&baseline, &list_properties(Some(&later_track), 180_000_000)),
            Properties::new()
        );
        let paused_track = Track {
            is_playing: false,
            ..later_track
        };
        assert_eq!(
            list_changes(
                &baseline,
                &list_properties(Some(&paused_track), 180_000_000)
            ),
            vec![
                ("PlaybackStatus", "Paused".to_variant()),
                ("Position", 33_000_000i64.to_variant()),
            ]
        );
    }

    #[test]
    fn buttons_reach_the_player_only_when_it_supports_them() {
        let track = build_playing_track();
        let cases = [
            ("Play", Some(Action::Play)),
            ("Pause", Some(Action::Pause)),
            ("PlayPause", Some(Action::PlayPause)),
            ("Stop", Some(Action::Stop)),
            ("Next", Some(Action::Next)),
            ("Previous", Some(Action::Previous)),
            ("Seek", None),
        ];
        for (method, action) in cases {
            assert_eq!(route(method, &track), action, "{method}");
        }
        let limited_track = Track {
            is_playing: false,
            can_play: false,
            can_go_next: false,
            can_go_previous: false,
            ..track.clone()
        };
        for method in ["Play", "PlayPause", "Next", "Previous"] {
            assert_eq!(route(method, &limited_track), None, "{method}");
        }
        assert_eq!(route("Pause", &limited_track), Some(Action::Pause));
        let seekable_track = Track {
            can_seek: true,
            length: 180_000_000,
            is_length_supported: true,
            ..limited_track.clone()
        };
        assert_eq!(route("Next", &seekable_track), Some(Action::Next));
        let uncontrolled_track = Track {
            can_control: false,
            ..track
        };
        assert_eq!(route("Stop", &uncontrolled_track), None);
    }
}
