pub mod soundcards;
pub mod volumelevels;

use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::inputs::slider_row;
use crate::panels::settings::content::{Context, Page};
use crate::panels::sound::devices::DevicePicker;
use crate::services::media::audio::{self, Audio};
use crate::ui::widgets::input::controls::ConfigSwitch;
use crate::ui::widgets::input::slider::Slider;

const PERCENT: (f64, f64) = (0.0, 100.0);
const PROTECTION: &str = "/audio/protection/enable";
const THEME: &str = "/sounds/theme";
const DEFAULT_THEME: &str = "freedesktop";
pub(super) const NO_SERVER: &str = "No PulseAudio-compatible sound server is running, so there is nothing to control. PipeWire provides one with the pipewire-pulse package.";
const NO_THEME: &str = "No sound theme is installed in /usr/share/sounds, so alert sounds stay silent. The default one comes with the sound-theme-freedesktop package.";

struct Side {
    is_sink: bool,
    picker: Rc<DevicePicker>,
    slider: Rc<Slider>,
    mute_switch: Rc<ConfigSwitch>,
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

    let more_section = page.section("", "");
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
            &more_section,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let alerts_section = page.section("notification_sound", &tr("Alert Sound"));
    if audio::sound_themes().is_empty() {
        page.notice(&alerts_section, "info", &tr(NO_THEME));
    }
    let first_row = page.uniform_row(&alerts_section);
    page.config_switch(
        &first_row,
        "battery_android_full",
        &tr("Battery"),
        "/sounds/battery",
        false,
    );
    page.config_switch(
        &first_row,
        "av_timer",
        &tr("Pomodoro"),
        "/sounds/pomodoro",
        false,
    );
    let second_row = page.uniform_row(&alerts_section);
    let microphone_switch = page.config_switch(
        &second_row,
        "mic",
        &tr("Microphone"),
        "/sounds/microphone",
        true,
    );
    page.tip(
        &microphone_switch.button,
        &tr("Played when the microphone is muted or unmuted"),
    );
    let usb_switch = page.config_switch(
        &second_row,
        "usb",
        &tr("USB devices"),
        "/sounds/devices",
        true,
    );
    page.tip(
        &usb_switch.button,
        &tr("Played when a USB device is connected or disconnected"),
    );
    let theme_section = page.subsection(&alerts_section, &tr("Sound theme"), "");
    let theme_combo = page.combo(&theme_section, "notification_sound");
    let theme_names = Rc::new(RefCell::new(Vec::<String>::new()));
    let show_themes = {
        let theme_combo = theme_combo.clone();
        let theme_names = theme_names.clone();
        move || {
            let current_theme =
                config::value_str(THEME).unwrap_or_else(|| DEFAULT_THEME.to_owned());
            let mut names = audio::sound_themes();
            if !current_theme.is_empty() && !names.contains(&current_theme) {
                names.insert(0, current_theme.clone());
            }
            let index = names
                .iter()
                .position(|name| *name == current_theme)
                .unwrap_or(0);
            theme_combo.set_items(&names, index as i32);
            theme_names.replace(names);
        }
    };
    show_themes();
    theme_combo.connect_activated(move |index| {
        if let Some(name) = theme_names.borrow().get(index) {
            config::store_value(THEME, Value::from(name.clone()));
        }
    });
    page.watch(THEME, show_themes);

    let protection_section = page.section("hearing", &tr("Earbang protection"));
    let enable_switch = page.config_switch(
        &protection_section,
        "hearing",
        &tr("Enable"),
        PROTECTION,
        false,
    );
    page.tip(
        &enable_switch.button,
        &tr("Prevents abrupt increments and restricts volume limit"),
    );
    let limits_row = page.row(&protection_section);
    let (increase_row, increase_spin) = page.config_spin(
        &limits_row,
        "arrow_warm_up",
        &tr("Max allowed increase"),
        "/audio/protection/maxAllowedIncrease",
        10,
        (0, 100),
        2,
    );
    let (limit_row, limit_spin) = page.config_spin(
        &limits_row,
        "vertical_align_top",
        &tr("Volume limit"),
        "/audio/protection/maxAllowed",
        99,
        (0, 154),
        2,
    );
    let update_protection_rows = move || {
        let is_enabled = config::value_bool(PROTECTION, false);
        Page::set_spin_row_enabled(&increase_row, &increase_spin, is_enabled);
        Page::set_spin_row_enabled(&limit_row, &limit_spin, is_enabled);
    };
    update_protection_rows();
    page.watch(PROTECTION, update_protection_rows);

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

fn side(page: &Page, audio: &Audio, is_sink: bool) -> Rc<Side> {
    let (icon, title, slider_icon, mute_icon) = if is_sink {
        ("volume_up", "Output", "volume_up", "volume_off")
    } else {
        ("mic", "Input", "mic", "mic_off")
    };
    let section = page.section(icon, &tr(title));
    let group = page.subsection(&section, &tr("Device"), "");
    let combo = page.combo(&group, if is_sink { "speaker" } else { "mic" });
    let (slider, _) = slider_row(&page.theme, &section, slider_icon, &tr("Volume"), PERCENT);
    let mute_switch = page.switch(&section, mute_icon, &tr("Mute"), {
        let audio = audio.clone();
        move |should_mute| {
            let is_muted = if is_sink {
                audio.is_sink_muted.get()
            } else {
                audio.is_source_muted.get()
            };
            if is_muted == should_mute {
                return;
            }
            if is_sink {
                audio.toggle_sink_mute();
            } else {
                audio.toggle_source_mute();
            }
        }
    });
    let side = Rc::new(Side {
        is_sink,
        picker: DevicePicker::new(combo, audio, is_sink),
        slider,
        mute_switch,
    });
    side.slider.on_moved({
        let audio = audio.clone();
        let slider = Rc::downgrade(&side.slider);
        move |value| {
            if let Some(slider) = slider.upgrade() {
                slider.set_tooltip(&format!("{}%", value.round()));
            }
            if is_sink {
                audio.set_sink_volume(value.round() / 100.0);
            } else {
                audio.set_source_volume(value.round() / 100.0);
            }
        }
    });
    side
}

fn refresh_side(audio: &Audio, side: &Rc<Side>) {
    let (volume, is_muted) = if side.is_sink {
        (audio.sink_volume.get(), audio.is_sink_muted.get())
    } else {
        (audio.source_volume.get(), audio.is_source_muted.get())
    };
    show_percent(&side.slider, (volume * 100.0).round());
    side.mute_switch.set(is_muted);
    side.picker.refresh();
}
