use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, slider_row};
use crate::services::audio::{self, Audio, Device};
use crate::ui::widgets::controls::{ComboBox, ConfigSwitch};
use crate::ui::widgets::slider::Slider;

const PERCENT: (f64, f64) = (0.0, 100.0);
const PROTECTION: &str = "/audio/protection/enable";
const THEME: &str = "/sounds/theme";
const DEFAULT_THEME: &str = "freedesktop";
pub(super) const NO_SERVER: &str = "No PulseAudio-compatible sound server is running, so there is nothing to control. PipeWire provides one with the pipewire-pulse package.";
const NO_THEME: &str = "No sound theme is installed in /usr/share/sounds, so alert sounds stay silent. The default one comes with the sound-theme-freedesktop package.";

struct Side {
    sink: bool,
    devices: RefCell<Vec<Device>>,
    combo: Rc<ComboBox>,
    slider: Rc<Slider>,
    mute: Rc<ConfigSwitch>,
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
        let section = page.section("volume_off", &tr("Sound"));
        page.notice(&section, "info", &tr(NO_SERVER));
        return page;
    };

    let output = side(&page, &audio, true);
    let input = side(&page, &audio, false);

    let more = page.section("", "");
    for (icon, title, subtitle, id) in [
        (
            "tune",
            "Volume Levels",
            "The volume of each app playing or recording",
            "volumelevels",
        ),
        (
            "speaker",
            "Sound cards",
            "Which input and output configuration each card uses",
            "soundcards",
        ),
    ] {
        page.link_row(
            &more,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let alerts = page.section("notification_sound", &tr("Alert Sound"));
    if audio::sound_themes().is_empty() {
        page.notice(&alerts, "info", &tr(NO_THEME));
    }
    let kinds = page.uniform_row(&alerts);
    page.config_switch(
        &kinds,
        "battery_android_full",
        &tr("Battery"),
        "/sounds/battery",
        false,
    );
    page.config_switch(
        &kinds,
        "av_timer",
        &tr("Pomodoro"),
        "/sounds/pomodoro",
        false,
    );
    let switches = page.uniform_row(&alerts);
    let microphone = page.config_switch(
        &switches,
        "mic",
        &tr("Microphone"),
        "/sounds/microphone",
        true,
    );
    page.tip(
        &microphone.button,
        &tr("Played when the microphone is muted or unmuted"),
    );
    let devices = page.config_switch(
        &switches,
        "usb",
        &tr("USB devices"),
        "/sounds/devices",
        true,
    );
    page.tip(
        &devices.button,
        &tr("Played when a USB device is connected or disconnected"),
    );
    let theme_group = page.subsection(&alerts, &tr("Sound theme"), "");
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

    let protection = page.section("hearing", &tr("Earbang protection"));
    let enable = page.config_switch(&protection, "hearing", &tr("Enable"), PROTECTION, false);
    page.tip(
        &enable.button,
        &tr("Prevents abrupt increments and restricts volume limit"),
    );
    let limits = page.row(&protection);
    let (increase_row, increase) = page.config_spin(
        &limits,
        "arrow_warm_up",
        &tr("Max allowed increase"),
        "/audio/protection/maxAllowedIncrease",
        10,
        (0, 100),
        2,
    );
    let (limit_row, limit) = page.config_spin(
        &limits,
        "vertical_align_top",
        &tr("Volume limit"),
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
        move || {
            for side in [output.upgrade(), input.upgrade()].into_iter().flatten() {
                refresh_side(&audio, &side);
            }
        }
    };
    refresh();
    page.keep(audio.watch(refresh));
    page.keep((output, input));
    page
}

fn side(page: &Page, audio: &Audio, sink: bool) -> Rc<Side> {
    let (icon, title, slider_icon, mute_icon) = if sink {
        ("volume_up", "Output", "volume_up", "volume_off")
    } else {
        ("mic", "Input", "mic", "mic_off")
    };
    let section = page.section(icon, &tr(title));
    let group = page.subsection(&section, &tr("Device"), "");
    let combo = page.combo(&group, if sink { "speaker" } else { "mic" });
    let (slider, _) = slider_row(&page.theme, &section, slider_icon, &tr("Volume"), PERCENT);
    let mute = page.switch(&section, mute_icon, &tr("Mute"), {
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
