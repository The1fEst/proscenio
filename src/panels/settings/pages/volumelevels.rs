use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::core::process;
use crate::panels::settings::content::{Context, Page, new_slider};
use crate::panels::settings::pages::sound::NO_SERVER;
use crate::platform::appicon;
use crate::services::audio::{Audio, Stream};
use crate::services::mpris::{Mpris, Track};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::coalesce;
use crate::ui::widgets::column::Column;
use crate::ui::widgets::slider::Slider;
use crate::ui::widgets::text;
use crate::ui::widgets::tooltip::Tooltip;

const PERCENT: (f64, f64) = (0.0, 100.0);
const EMPTY_MARGIN: i32 = 8;
const ROW_SPACING: i32 = 12;
const ROW_MARGIN: i32 = 8;
const COLUMN_SPACING: i32 = 10;
const LINE_SPACING: i32 = 2;
const ICON_SIZE: i32 = 36;
const MUTE_SIZE: f64 = 22.0;
const MUTED_OPACITY: f64 = 0.4;

struct StreamRow {
    index: u32,
    ancestors: Vec<u32>,
    is_muted: Rc<Cell<bool>>,
    icon: gtk4::Image,
    mute: gtk4::Label,
    tip: Rc<Tooltip>,
    title: gtk4::Label,
    slider: Rc<Slider>,
}

struct StreamList {
    sink: bool,
    empty: gtk4::Label,
    rows: gtk4::Box,
    shown: RefCell<Vec<StreamRow>>,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let Some(audio) = context.services.audio.clone() else {
        let section = page.section("volume_off", &tr("Sound"));
        page.notice(&section, "info", &tr(NO_SERVER));
        return page;
    };
    let mpris = context.services.mpris.clone();

    let lists = [add_list(&page, true), add_list(&page, false)];
    let refresh = {
        let (audio, mpris) = (audio.clone(), mpris.clone());
        let page = Rc::downgrade(&page);
        let lists = lists.each_ref().map(Rc::downgrade);
        move || {
            let Some(page) = page.upgrade() else {
                return;
            };
            for list in lists.iter().filter_map(Weak::upgrade) {
                refresh_list(&page, &audio, &mpris, &list);
            }
        }
    };
    refresh();
    let queued = coalesce(Rc::new(refresh));
    page.keep(audio.watch({
        let queued = queued.clone();
        move || queued()
    }));
    page.keep(mpris.subscribe(move || queued()));
    page.keep(lists);
    page
}

fn add_list(page: &Page, sink: bool) -> Rc<StreamList> {
    let (icon, title, nothing) = if sink {
        ("volume_up", "Playback", "Nothing is playing")
    } else {
        ("mic", "Recording", "Nothing is recording")
    };
    let section = page.section(icon, &tr(title));
    let empty = text::styled(&tr(nothing));
    text::set_color(&empty, "colSubtext");
    empty.set_xalign(0.0);
    empty.set_margin_start(EMPTY_MARGIN);
    section.append(&empty);
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, ROW_SPACING);
    section.append(&rows);
    Rc::new(StreamList {
        sink,
        empty,
        rows,
        shown: RefCell::new(Vec::new()),
    })
}

fn refresh_list(page: &Rc<Page>, audio: &Audio, mpris: &Mpris, list: &Rc<StreamList>) {
    let (page, weak, sink) = (Rc::downgrade(page), Rc::downgrade(list), list.sink);
    let (owner, mpris) = (audio.clone(), mpris.clone());
    audio.streams(sink, move |streams| {
        let (Some(page), Some(list)) = (page.upgrade(), weak.upgrade()) else {
            return;
        };
        list.empty.set_visible(streams.is_empty());
        list.rows.set_visible(!streams.is_empty());
        let same = list
            .shown
            .borrow()
            .iter()
            .map(|row| row.index)
            .eq(streams.iter().map(|stream| stream.index));
        if !same {
            while let Some(child) = list.rows.first_child() {
                list.rows.remove(&child);
            }
            let rows = streams
                .iter()
                .map(|stream| add_row(&page, &owner, sink, &list.rows, stream))
                .collect();
            list.shown.replace(rows);
        }
        let players = mpris.players.borrow();
        for (row, stream) in list.shown.borrow().iter().zip(&streams) {
            show(row, stream, &players);
        }
    });
}

fn add_row(
    page: &Page,
    audio: &Audio,
    sink: bool,
    parent: &gtk4::Box,
    stream: &Stream,
) -> StreamRow {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, COLUMN_SPACING);
    row.set_margin_start(ROW_MARGIN);
    row.set_margin_end(ROW_MARGIN);

    let icon = gtk4::Image::new();
    icon.set_pixel_size(ICON_SIZE);
    if let Some(display) = gtk4::gdk::Display::default() {
        let icons = gtk4::IconTheme::for_display(&display);
        let preferred = appicon::guess(&icons, &stream.icon);
        let name = if !stream.icon.is_empty() && icons.has_icon(&preferred) {
            preferred
        } else {
            appicon::guess(&icons, &stream.node)
        };
        icon.set_icon_name(Some(&name));
    }
    let mute = text::symbol(if sink { "volume_off" } else { "mic_off" }, MUTE_SIZE);
    text::set_color(&mute, "colOnLayer1");
    let badge = gtk4::Overlay::new();
    badge.set_child(Some(&icon));
    badge.add_overlay(&Centred::new(&mute));
    badge.set_size_request(ICON_SIZE, ICON_SIZE);
    badge.set_valign(gtk4::Align::Center);
    badge.set_cursor_from_name(Some("pointer"));
    let is_muted = Rc::new(Cell::new(stream.muted));
    let click = gtk4::GestureClick::new();
    click.connect_released({
        let (audio, is_muted, index) = (audio.clone(), is_muted.clone(), stream.index);
        move |_, _, _, _| audio.set_stream_mute(sink, index, !is_muted.get())
    });
    badge.add_controller(click);
    let tip = page.unkept_tip(&badge, "");
    row.append(&badge);

    let lines = Column::filling_width(LINE_SPACING);
    lines.set_hexpand(true);
    let title = text::styled("");
    text::set_color(&title, "colOnSecondaryContainer");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&title);
    let slider = new_slider(&page.theme, PERCENT);
    slider.on_moved({
        let (audio, index) = (audio.clone(), stream.index);
        let slider = Rc::downgrade(&slider);
        move |value| {
            if let Some(slider) = slider.upgrade() {
                slider.set_tooltip(&format!("{}%", value.round()));
            }
            audio.set_stream_volume(sink, index, value.round() / 100.0);
        }
    });
    lines.append(&slider.area);
    row.append(&lines);
    parent.append(&row);

    StreamRow {
        index: stream.index,
        ancestors: stream.pid.map(process::ancestors).unwrap_or_default(),
        is_muted,
        icon,
        mute,
        tip,
        title,
        slider,
    }
}

fn title_of(stream: &Stream, ancestors: &[u32], players: &[Track]) -> String {
    let player_title = players
        .iter()
        .filter(|track| !track.title.is_empty())
        .filter(|track| track.pid.is_some_and(|pid| ancestors.contains(&pid)))
        .max_by_key(|track| track.playing)
        .map(|track| track.title.as_str());
    match player_title.or(stream.media.as_deref()) {
        Some(title) if !title.is_empty() && title != stream.name => {
            format!("{} • {title}", stream.name)
        }
        _ => stream.name.clone(),
    }
}

fn show(row: &StreamRow, stream: &Stream, players: &[Track]) {
    row.title
        .set_text(&title_of(stream, &row.ancestors, players));
    let percent = (stream.volume * 100.0).round();
    row.slider.set(percent);
    row.slider.set_tooltip(&format!("{percent}%"));
    row.is_muted.set(stream.muted);
    row.mute.set_visible(stream.muted);
    row.icon
        .set_opacity(if stream.muted { MUTED_OPACITY } else { 1.0 });
    if stream.muted {
        row.icon.add_css_class("desaturated");
    } else {
        row.icon.remove_css_class("desaturated");
    }
    row.tip.set_text(&tr(if stream.muted {
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
            muted: false,
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
            playing,
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
        assert_eq!(title_of(&brave, &[300], &players), "Brave • Playback");
        assert_eq!(
            title_of(&brave, &[100], &[player(100, "", true)]),
            "Brave • Playback"
        );
    }
}
