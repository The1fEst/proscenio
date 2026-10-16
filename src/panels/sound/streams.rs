use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::process;
use crate::panels::settings::content::inputs::new_slider;
use crate::platform::system::appicon;
use crate::services::media::audio::{Audio, Stream};
use crate::services::media::mpris::{Mpris, Track};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::slider::Slider;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::column::Column;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

pub const PERCENT: (f64, f64) = (0.0, 100.0);
pub const ROW_MARGIN: i32 = 8;
pub const COLUMN_SPACING: i32 = 10;
pub const LINE_SPACING: i32 = 2;
pub const ICON_SIZE: i32 = 36;
const EMPTY_MARGIN: i32 = 8;
const ROW_SPACING: i32 = 12;
const MUTE_SIZE: f64 = 22.0;
const MUTED_OPACITY: f64 = 0.4;
const GENERIC_NAMES: &[&str] = &[
    "playback",
    "playbackstream",
    "audioplayback",
    "alsaplayback",
    "record",
    "recording",
    "recordstream",
    "capture",
    "capturestream",
    "alsacapture",
    "audiostream",
    "audio",
    "audiooutput",
    "audioinput",
    "stream",
    "output",
    "input",
    "sound",
    "simpledirectmedialayer",
    "unknown",
];

struct StreamRow {
    index: u32,
    ancestors: Vec<u32>,
    is_muted: Rc<Cell<bool>>,
    icon: gtk4::Image,
    mute_label: gtk4::Label,
    tip: Rc<Tooltip>,
    title_label: gtk4::Label,
    slider: Rc<Slider>,
}

pub struct StreamList {
    pub root: gtk4::Box,
    is_sink: bool,
    theme: SharedTheme,
    audio: Audio,
    mpris: Mpris,
    empty_label: gtk4::Label,
    row_box: gtk4::Box,
    rows: RefCell<Vec<StreamRow>>,
    on_updated: RefCell<Option<Rc<dyn Fn()>>>,
}

impl StreamList {
    pub fn new(theme: &SharedTheme, audio: &Audio, mpris: &Mpris, is_sink: bool) -> Rc<Self> {
        let empty_text = if is_sink {
            "Nothing is playing"
        } else {
            "Nothing is recording"
        };
        let empty_label = text::styled(&tr(empty_text));
        text::set_color(&empty_label, "colSubtext");
        empty_label.set_xalign(0.0);
        empty_label.set_margin_start(EMPTY_MARGIN);
        let row_box = gtk4::Box::new(gtk4::Orientation::Vertical, ROW_SPACING);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&empty_label);
        root.append(&row_box);
        Rc::new(StreamList {
            root,
            is_sink,
            theme: theme.clone(),
            audio: audio.clone(),
            mpris: mpris.clone(),
            empty_label,
            row_box,
            rows: RefCell::new(Vec::new()),
            on_updated: RefCell::new(None),
        })
    }

    pub fn connect_updated(&self, action: impl Fn() + 'static) {
        self.on_updated.replace(Some(Rc::new(action)));
    }

    pub fn refresh(self: &Rc<Self>) {
        let list = Rc::downgrade(self);
        self.audio.read_streams(self.is_sink, move |streams| {
            if let Some(list) = list.upgrade() {
                list.update(&streams);
            }
        });
    }

    fn update(&self, streams: &[Stream]) {
        self.empty_label.set_visible(streams.is_empty());
        self.row_box.set_visible(!streams.is_empty());
        let has_same_rows =
            has_same_streams(self.rows.borrow().iter().map(|row| row.index), streams);
        if !has_same_rows {
            while let Some(child) = self.row_box.first_child() {
                self.row_box.remove(&child);
            }
            let rows = streams.iter().map(|stream| self.add_row(stream)).collect();
            self.rows.replace(rows);
        }
        {
            let players = self.mpris.players.borrow();
            for (row, stream) in self.rows.borrow().iter().zip(streams) {
                show(row, stream, &players);
            }
        }
        let on_updated = self.on_updated.borrow().clone();
        if let (false, Some(on_updated)) = (has_same_rows, on_updated) {
            on_updated();
        }
    }

    fn add_row(&self, stream: &Stream) -> StreamRow {
        let (audio, is_sink) = (&self.audio, self.is_sink);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, COLUMN_SPACING);
        row.set_margin_start(ROW_MARGIN);
        row.set_margin_end(ROW_MARGIN);

        let icon = gtk4::Image::new();
        icon.set_pixel_size(ICON_SIZE);
        if let Some(display) = gtk4::gdk::Display::default() {
            let icon_theme = gtk4::IconTheme::for_display(&display);
            let preferred_name = appicon::guess(&icon_theme, &stream.icon);
            let name = if !stream.icon.is_empty() && icon_theme.has_icon(&preferred_name) {
                preferred_name
            } else {
                appicon::guess(&icon_theme, &stream.node)
            };
            icon.set_icon_name(Some(&name));
        }
        let mute_label = text::symbol(if is_sink { "volume_off" } else { "mic_off" }, MUTE_SIZE);
        text::set_color(&mute_label, "colOnLayer1");
        let badge = gtk4::Overlay::new();
        badge.set_child(Some(&icon));
        badge.add_overlay(&Centered::new(&mute_label));
        badge.set_size_request(ICON_SIZE, ICON_SIZE);
        badge.set_valign(gtk4::Align::Center);
        badge.set_cursor_from_name(Some("pointer"));
        let is_muted = Rc::new(Cell::new(stream.is_muted));
        let click_gesture = gtk4::GestureClick::new();
        click_gesture.connect_released({
            let (audio, is_muted, index) = (audio.clone(), is_muted.clone(), stream.index);
            move |_, _, _, _| audio.set_stream_mute(is_sink, index, !is_muted.get())
        });
        badge.add_controller(click_gesture);
        let tip = tooltip::attach_hover_tip(&badge, &self.theme, "");
        row.append(&badge);

        let lines = Column::filling_width(LINE_SPACING);
        lines.set_hexpand(true);
        let title_label = text::styled("");
        text::set_color(&title_label, "colOnSecondaryContainer");
        title_label.set_xalign(0.0);
        title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&title_label);
        let slider = new_slider(&self.theme, PERCENT);
        slider.on_moved({
            let (audio, index) = (audio.clone(), stream.index);
            let slider = Rc::downgrade(&slider);
            move |value| {
                if let Some(slider) = slider.upgrade() {
                    slider.set_tooltip(&format!("{}%", value.round()));
                }
                audio.set_stream_volume(is_sink, index, value.round() / 100.0);
            }
        });
        lines.append(&slider.area);
        row.append(&lines);
        self.row_box.append(&row);

        StreamRow {
            index: stream.index,
            ancestors: stream.pid.map(process::ancestors).unwrap_or_default(),
            is_muted,
            icon,
            mute_label,
            tip,
            title_label,
            slider,
        }
    }
}

fn has_same_streams(current: impl Iterator<Item = u32>, streams: &[Stream]) -> bool {
    current.eq(streams.iter().map(|stream| stream.index))
}

fn title_of(stream: &Stream, ancestors: &[u32], players: &[Track]) -> String {
    let player_title = players
        .iter()
        .filter(|track| !track.title.is_empty())
        .filter(|track| track.pid.is_some_and(|pid| ancestors.contains(&pid)))
        .max_by_key(|track| track.is_playing)
        .map(|track| track.title.as_str());
    let media_title = [player_title, stream.media.as_deref()]
        .into_iter()
        .flatten()
        .find(|title| {
            is_readable_text(title) && !is_repeat_of(title, &[&stream.name, &stream.node])
        });
    match media_title {
        Some(title) => format!("{} • {title}", stream.name),
        None => stream.name.clone(),
    }
}

fn is_readable_text(text: &str) -> bool {
    let is_identifier = text.is_ascii()
        && !text.contains(char::is_whitespace)
        && (text.contains(['.', '_', ':', '/']) || has_digit_beside_letter(text));
    !text.is_empty() && !is_identifier && !GENERIC_NAMES.contains(&to_generic_key(text).as_str())
}

fn has_digit_beside_letter(text: &str) -> bool {
    text.as_bytes().windows(2).any(|pair| {
        pair.iter().all(u8::is_ascii_alphanumeric)
            && pair[0].is_ascii_digit() != pair[1].is_ascii_digit()
    })
}

fn normalize_alphanumeric(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn to_generic_key(text: &str) -> String {
    normalize_alphanumeric(text)
        .trim_end_matches(|character: char| character.is_ascii_digit())
        .to_owned()
}

fn is_repeat_of(title: &str, names: &[&str]) -> bool {
    let key = normalize_alphanumeric(title);
    names
        .iter()
        .any(|name| *name == title || (!key.is_empty() && normalize_alphanumeric(name) == key))
}

fn show(row: &StreamRow, stream: &Stream, players: &[Track]) {
    row.title_label
        .set_text(&title_of(stream, &row.ancestors, players));
    let percent = (stream.volume * 100.0).round();
    row.slider.set(percent);
    row.slider.set_tooltip(&format!("{percent}%"));
    row.is_muted.set(stream.is_muted);
    row.mute_label.set_visible(stream.is_muted);
    row.icon
        .set_opacity(if stream.is_muted { MUTED_OPACITY } else { 1.0 });
    if stream.is_muted {
        row.icon.add_css_class("desaturated");
    } else {
        row.icon.remove_css_class("desaturated");
    }
    row.tip.set_text(&tr(if stream.is_muted {
        "Click to unmute"
    } else {
        "Click to mute"
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(name: &str, media: Option<&str>) -> Stream {
        Stream {
            index: 0,
            name: name.to_owned(),
            media: media.map(str::to_owned),
            icon: String::new(),
            node: String::new(),
            pid: None,
            volume: 1.0,
            is_muted: false,
        }
    }

    #[test]
    fn the_media_title_follows_the_app_name_unless_it_repeats_it() {
        for (name, media, title) in [
            ("Player", Some("Song - Artist"), "Player • Song - Artist"),
            ("Recorder", Some("Recorder"), "Recorder"),
            ("Recorder", Some(""), "Recorder"),
            ("Firefox", None, "Firefox"),
        ] {
            assert_eq!(title_of(&stream(name, media), &[], &[]), title);
        }
    }

    #[test]
    fn a_player_in_the_stream_process_or_its_parents_names_the_stream() {
        let player = |pid: u32, title: &str, playing: bool| Track {
            pid: Some(pid),
            title: title.to_owned(),
            is_playing: playing,
            ..Track::default()
        };
        let brave = stream("Brave", Some("Playback"));
        let players = [
            player(900, "Elsewhere", true),
            player(100, "Paused video", false),
            player(100, "Live final", true),
        ];
        assert_eq!(
            title_of(&brave, &[300, 200, 100], &players),
            "Brave • Live final"
        );
        assert_eq!(title_of(&brave, &[300], &players), "Brave");
        assert_eq!(title_of(&brave, &[100], &[player(100, "", true)]), "Brave");
        let named = stream("Brave", Some("Видео про Rust"));
        assert_eq!(
            title_of(&named, &[100], &[player(100, "org.brave.title", true)]),
            "Brave • Видео про Rust"
        );
        assert_eq!(
            title_of(
                &stream("Firefox", Some("Song - Artist")),
                &[100],
                &[player(100, "firefox", true)]
            ),
            "Firefox • Song - Artist"
        );
        let fallback = stream("Brave", Some("Media name"));
        assert_eq!(
            title_of(
                &fallback,
                &[100],
                &[
                    player(100, "Playback", true),
                    player(100, "Paused video", false)
                ]
            ),
            "Brave • Media name"
        );
        assert_eq!(title_of(&stream("♪", Some("♪")), &[], &[]), "♪");
    }

    #[test]
    fn every_generic_name_is_written_as_the_key_it_is_matched_by() {
        for name in GENERIC_NAMES {
            assert_eq!(to_generic_key(name), *name);
        }
    }

    #[test]
    fn a_media_part_shows_only_when_it_reads_as_human_text() {
        let mut firefox = stream("Firefox", Some("firefox"));
        firefox.node = "firefox".to_owned();
        assert_eq!(title_of(&firefox, &[], &[]), "Firefox");
        for (media, shown) in [
            ("ts.pa.dummy.playbackdefaultrender1", false),
            ("org.mozilla.firefox", false),
            ("/usr/bin/player", false),
            ("my_stream", false),
            ("stream:3", false),
            ("playbackdefaultrender1", false),
            ("Playback", false),
            ("playback", false),
            ("Playback Stream", false),
            ("Playback Stream #2", false),
            ("AudioStream", false),
            ("Audio Stream", false),
            ("ALSA Playback", false),
            ("ALSA Capture", false),
            ("Simple DirectMedia Layer", false),
            ("Output", false),
            ("sound", false),
            ("Record", false),
            ("TEAMSPEAK 3", false),
            ("Team Speak 3", false),
            ("Song - Artist", true),
            ("Despacito", true),
            ("Song 2", true),
            ("Intro (feat. Someone)", true),
            ("Видео про Rust", true),
            ("Привет.", true),
            ("♪", true),
            ("第1話", true),
        ] {
            let title = title_of(&stream("TeamSpeak 3", Some(media)), &[], &[]);
            let expected = if shown {
                format!("TeamSpeak 3 • {media}")
            } else {
                "TeamSpeak 3".to_owned()
            };
            assert_eq!(title, expected, "{media}");
        }
    }

    #[test]
    fn rows_are_kept_only_while_the_same_streams_stay_in_the_same_order() {
        let streams: Vec<Stream> = [4, 7, 9]
            .into_iter()
            .map(|index| Stream {
                index,
                ..stream("App", None)
            })
            .collect();
        let same = |current: &[u32]| has_same_streams(current.iter().copied(), &streams);
        assert!(same(&[4, 7, 9]));
        assert!(!same(&[4, 7]));
        assert!(!same(&[4, 7, 9, 12]));
        assert!(!same(&[7, 4, 9]));
        assert!(!same(&[]));
        assert!(has_same_streams(std::iter::empty(), &[]));
    }
}
