use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;
use crate::panels::settings::content::{Context, Page, slider_row};
use crate::services::audio::{self, Audio, Card, Device, Stream};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::controls::{ComboBox, ConfigSwitch};
use crate::ui::widgets::slider::Slider;
use crate::ui::widgets::text;

const PERCENT: (f64, f64) = (0.0, 100.0);
const EMPTY_MARGIN: i32 = 8;
const PROTECTION: &str = "/audio/protection/enable";
const THEME: &str = "/sounds/theme";
const DEFAULT_THEME: &str = "freedesktop";

struct Side {
    sink: bool,
    devices: RefCell<Vec<Device>>,
    combo: Rc<ComboBox>,
    slider: Rc<Slider>,
    mute: Rc<ConfigSwitch>,
}

struct Levels {
    empty: gtk4::Label,
    rows: gtk4::Box,
    shown: RefCell<Vec<(u32, Rc<Slider>, gtk4::Label)>>,
}

pub fn device_label(device: &Device) -> String {
    let (description, nick) = (&device.description, &device.nick);
    if !nick.is_empty() && description.len() > nick.len() && description.starts_with(nick.as_str())
    {
        return format!("{nick} · {}", description[nick.len()..].trim());
    }
    [description, nick, &device.name]
        .into_iter()
        .find(|text| !text.is_empty())
        .cloned()
        .unwrap_or_default()
}

fn show_percent(slider: &Slider, value: f64) {
    slider.set(value);
    slider.set_tooltip(&format!("{}%", value.round()));
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let Some(audio) = context.services.audio.clone() else {
        return page;
    };

    let output = side(&page, &audio, true);
    let input = side(&page, &audio, false);

    let levels_section = page.section("tune", "Volume Levels");
    let empty = text::styled("Nothing is playing");
    text::set_color(&empty, "colSubtext");
    empty.set_xalign(0.0);
    empty.set_margin_start(EMPTY_MARGIN);
    levels_section.append(&empty);
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    levels_section.append(&rows);
    let levels = Rc::new(Levels {
        empty,
        rows,
        shown: RefCell::new(Vec::new()),
    });

    let alerts = page.section("notification_sound", "Alert Sound");
    let kinds = page.uniform_row(&alerts);
    page.config_switch(
        &kinds,
        "battery_android_full",
        "Battery",
        "/sounds/battery",
        false,
    );
    page.config_switch(&kinds, "av_timer", "Pomodoro", "/sounds/pomodoro", false);
    let microphone = page.config_switch(&kinds, "mic", "Microphone", "/sounds/microphone", true);
    page.tip(
        &microphone.button,
        "Played when the microphone is muted or unmuted",
    );
    let theme_group = page.subsection(&alerts, "Sound theme", "");
    let themes = page.combo(&theme_group, "notification_sound");
    let theme_names = Rc::new(RefCell::new(Vec::<String>::new()));
    let show_themes = {
        let themes = themes.clone();
        let theme_names = theme_names.clone();
        move || {
            let current = config::value_str(THEME).unwrap_or_else(|| DEFAULT_THEME.to_owned());
            let mut names = audio::sound_themes();
            if !current.is_empty() && !names.contains(&current) {
                names.insert(0, current.clone());
            }
            let index = names.iter().position(|name| *name == current).unwrap_or(0);
            themes.set_items(&names, index as i32);
            theme_names.replace(names);
        }
    };
    show_themes();
    themes.connect_activated(move |index| {
        if let Some(name) = theme_names.borrow().get(index) {
            config::store_value(THEME, Value::from(name.clone()));
        }
    });
    page.watch(THEME, show_themes);

    let cards_section = page.section("speaker", "Sound cards");
    let cards_holder = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    cards_section.append(&cards_holder);
    load_cards(&page, &cards_holder);

    let protection = page.section("hearing", "Earbang protection");
    let enable = page.config_switch(&protection, "hearing", "Enable", PROTECTION, false);
    page.tip(
        &enable.button,
        "Prevents abrupt increments and restricts volume limit",
    );
    let limits = page.row(&protection);
    let (increase_row, increase) = page.config_spin(
        &limits,
        "arrow_warm_up",
        "Max allowed increase",
        "/audio/protection/maxAllowedIncrease",
        10,
        (0, 100),
        2,
    );
    let (limit_row, limit) = page.config_spin(
        &limits,
        "vertical_align_top",
        "Volume limit",
        "/audio/protection/maxAllowed",
        99,
        (0, 154),
        2,
    );
    let follow_protection = move || {
        let enabled = config::value_bool(PROTECTION, false);
        Page::set_spin_row_enabled(&increase_row, &increase, enabled);
        Page::set_spin_row_enabled(&limit_row, &limit, enabled);
    };
    follow_protection();
    page.watch(PROTECTION, follow_protection);

    let refresh = {
        let audio = audio.clone();
        let (output, input) = (Rc::downgrade(&output), Rc::downgrade(&input));
        let levels = Rc::downgrade(&levels);
        let theme = page.theme.clone();
        move || {
            for side in [output.upgrade(), input.upgrade()].into_iter().flatten() {
                refresh_side(&audio, &side);
            }
            if let Some(levels) = levels.upgrade() {
                refresh_levels(&theme, &audio, &levels);
            }
        }
    };
    refresh();
    page.keep(audio.watch(refresh));
    page.keep((output, input, levels));
    page
}

fn side(page: &Page, audio: &Audio, sink: bool) -> Rc<Side> {
    let (icon, title, slider_icon, mute_icon) = if sink {
        ("volume_up", "Output", "volume_up", "volume_off")
    } else {
        ("mic", "Input", "mic", "mic_off")
    };
    let section = page.section(icon, title);
    let group = page.subsection(&section, "Device", "");
    let combo = page.combo(&group, if sink { "speaker" } else { "mic" });
    let (slider, _) = slider_row(&page.theme, &section, slider_icon, "Volume", PERCENT);
    let mute = page.switch(&section, mute_icon, "Mute", {
        let audio = audio.clone();
        move |wanted| {
            let muted = if sink {
                audio.sink_muted.get()
            } else {
                audio.source_muted.get()
            };
            if muted == wanted {
                return;
            }
            if sink {
                audio.toggle_sink_mute();
            } else {
                audio.toggle_source_mute();
            }
        }
    });
    let side = Rc::new(Side {
        sink,
        devices: RefCell::new(Vec::new()),
        combo,
        slider,
        mute,
    });
    side.combo.connect_activated({
        let side = Rc::downgrade(&side);
        let audio = audio.clone();
        move |index| {
            let Some(side) = side.upgrade() else {
                return;
            };
            if let Some(device) = side.devices.borrow().get(index) {
                audio.set_default(sink, &device.name);
            }
        }
    });
    side.slider.on_moved({
        let audio = audio.clone();
        let slider = Rc::downgrade(&side.slider);
        move |value| {
            if let Some(slider) = slider.upgrade() {
                slider.set_tooltip(&format!("{}%", value.round()));
            }
            if sink {
                audio.set_sink_volume(value.round() / 100.0);
            } else {
                audio.set_source_volume(value.round() / 100.0);
            }
        }
    });
    side
}

fn refresh_side(audio: &Audio, side: &Rc<Side>) {
    let (volume, muted) = if side.sink {
        (audio.sink_volume.get(), audio.sink_muted.get())
    } else {
        (audio.source_volume.get(), audio.source_muted.get())
    };
    show_percent(&side.slider, (volume * 100.0).round());
    side.mute.set(muted);
    let weak = Rc::downgrade(side);
    audio.devices(side.sink, move |devices, current| {
        let Some(side) = weak.upgrade() else {
            return;
        };
        let labels: Vec<String> = devices.iter().map(device_label).collect();
        let index = devices
            .iter()
            .position(|device| device.name == current)
            .unwrap_or(0);
        side.combo.set_items(&labels, index as i32);
        side.devices.replace(devices);
    });
}

fn refresh_levels(theme: &SharedTheme, audio: &Audio, levels: &Rc<Levels>) {
    let theme = theme.clone();
    let audio_for_rows = audio.clone();
    let weak = Rc::downgrade(levels);
    audio.streams(true, move |streams: Vec<Stream>| {
        let Some(levels) = weak.upgrade() else {
            return;
        };
        levels.empty.set_visible(streams.is_empty());
        levels.rows.set_visible(!streams.is_empty());
        let indices: Vec<u32> = streams.iter().map(|stream| stream.index).collect();
        let same = levels
            .shown
            .borrow()
            .iter()
            .map(|(index, _, _)| *index)
            .eq(indices.iter().copied());
        if !same {
            while let Some(child) = levels.rows.first_child() {
                levels.rows.remove(&child);
            }
            let mut shown = Vec::new();
            for stream in &streams {
                let (slider, symbol) =
                    slider_row(&theme, &levels.rows, "volume_up", &stream.name, PERCENT);
                slider.on_moved({
                    let audio = audio_for_rows.clone();
                    let index = stream.index;
                    let tip = Rc::downgrade(&slider);
                    move |value| {
                        if let Some(slider) = tip.upgrade() {
                            slider.set_tooltip(&format!("{}%", value.round()));
                        }
                        audio.set_stream_volume(true, index, value.round() / 100.0);
                    }
                });
                shown.push((stream.index, slider, symbol));
            }
            levels.shown.replace(shown);
        }
        for ((_, slider, symbol), stream) in levels.shown.borrow().iter().zip(&streams) {
            show_percent(slider, (stream.volume * 100.0).round());
            symbol.set_text(if stream.muted {
                "volume_off"
            } else {
                "volume_up"
            });
        }
    });
}

type Held = Rc<RefCell<Vec<Box<dyn Any>>>>;

fn load_cards(page: &Rc<Page>, holder: &gtk4::Box) {
    let held: Held = Rc::default();
    page.keep(held.clone());
    let (page, holder, held) = (
        Rc::downgrade(page),
        holder.downgrade(),
        Rc::downgrade(&held),
    );
    audio::cards(move |cards: Vec<Card>| {
        if let (Some(page), Some(holder), Some(held)) =
            (page.upgrade(), holder.upgrade(), held.upgrade())
        {
            fill_cards(&page, &holder, &held, cards);
        }
    });
}

fn fill_cards(page: &Rc<Page>, holder: &gtk4::Box, held: &Held, cards: Vec<Card>) {
    while let Some(child) = holder.first_child() {
        holder.remove(&child);
    }
    let mut kept = held.borrow_mut();
    kept.clear();
    for card in cards {
        let (group, tip) = page.unkept_subsection(
            holder,
            &card.description,
            "Which of the card's input and output configurations PipeWire uses",
        );
        let combo = ComboBox::new(&page.theme);
        combo.set_icon("tune");
        combo.button.set_hexpand(true);
        group.append(&combo.button);
        let labels: Vec<String> = card
            .profiles
            .iter()
            .map(|(label, _)| label.clone())
            .collect();
        let index = card
            .profiles
            .iter()
            .position(|(_, value)| *value == card.active)
            .unwrap_or(0);
        combo.set_items(&labels, index as i32);
        combo.connect_activated({
            let (page, holder) = (Rc::downgrade(page), holder.downgrade());
            let held = Rc::downgrade(held);
            let name = card.name.clone();
            let profiles = card.profiles.clone();
            move |index| {
                let Some((_, profile)) = profiles.get(index) else {
                    return;
                };
                let (page, holder, held) = (page.clone(), holder.clone(), held.clone());
                audio::set_card_profile(&name, profile, move || {
                    audio::cards(move |cards| {
                        if let (Some(page), Some(holder), Some(held)) =
                            (page.upgrade(), holder.upgrade(), held.upgrade())
                        {
                            fill_cards(&page, &holder, &held, cards);
                        }
                    });
                });
            }
        });
        kept.push(Box::new(combo));
        kept.push(Box::new(tip));
    }
}
