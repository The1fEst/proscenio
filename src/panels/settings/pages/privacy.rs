use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config::{self, Config};
use crate::core::i18n::tr;
use crate::core::tools;
use crate::panels::settings::content::{Context, Page, Style};
use crate::services::privacy;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text;

const TILE_HEIGHT: i32 = 52;
const TILE_SPACING: i32 = 10;
const POLL: Duration = Duration::from_secs(2);

pub struct Tile {
    pub icon: gtk4::Label,
    pub label: gtk4::Label,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let system = page.section("tune", &tr("System"));
    page.link_row(
        &system,
        "lock",
        &tr("Screen Lock"),
        &tr("What the lock screen looks like and what it lets through"),
        context.subpage_opener("lock"),
    );
    page.link_row(
        &system,
        "screenshot_frame_2",
        &tr("Screenshots & Recording"),
        &tr("Where captures go and how the region selector behaves"),
        context.subpage_opener("capture"),
    );

    let devices = page.section("sensors", &tr("Devices"));
    page.tools_notice(
        &devices,
        &[&tools::PW_DUMP],
        &tr("the microphone and screen always read as unused"),
    );
    let row = page.uniform_row(&devices);
    let microphone = tile(&row, "mic", &tr("Microphone idle"));
    let screen = tile(&row, "screen_share", &tr("Screen not shared"));
    page.every(POLL, move || {
        let microphone = (microphone.icon.clone(), microphone.label.clone());
        let screen = (screen.icon.clone(), screen.label.clone());
        privacy::read(move |activity| {
            show(
                &microphone,
                activity.microphone,
                &tr("Microphone in use"),
                &tr("Microphone idle"),
            );
            show(
                &screen,
                activity.screen,
                &tr("Screen being shared"),
                &tr("Screen not shared"),
            );
        });
    });

    let safety = page.section("work_alert", &tr("Work safety"));
    page.config_switch(
        &safety,
        "assignment",
        &tr("Hide clipboard images copied from sussy sources"),
        "/workSafety/enable/clipboard",
        false,
    );
    page.config_switch(
        &safety,
        "wallpaper",
        &tr("Hide sussy/anime wallpapers"),
        "/workSafety/enable/wallpaper",
        false,
    );
    let keywords = page.subsection(
        &safety,
        &tr("Trigger keywords"),
        &tr("Comma-separated. Work safety kicks in when one of these shows up."),
    );
    keyword_field(
        &page,
        &keywords,
        &tr("Network names"),
        "/workSafety/triggerCondition/networkNameKeywords",
        |config| &config.safety_networks,
    );
    keyword_field(
        &page,
        &keywords,
        &tr("File names"),
        "/workSafety/triggerCondition/fileKeywords",
        |config| &config.safety_files,
    );
    keyword_field(
        &page,
        &keywords,
        &tr("Links"),
        "/workSafety/triggerCondition/linkKeywords",
        |config| &config.safety_links,
    );
    page
}

pub fn tile(row: &gtk4::Box, symbol: &str, caption: &str) -> Tile {
    let tile = gtk4::Box::new(gtk4::Orientation::Horizontal, TILE_SPACING);
    tile.add_css_class("settings-tile");
    tile.set_size_request(-1, TILE_HEIGHT);
    let icon = text::symbol(symbol, pixel_size::LARGER as f64);
    text::set_color(&icon, "colSubtext");
    let icon_box = Centred::integral(&icon);
    tile.append(&icon_box);
    let label = text::styled(caption);
    text::set_color(&label, "colOnLayer2");
    label.set_xalign(0.0);
    let label_box = Centred::filling_width(&label);
    label_box.set_hexpand(true);
    tile.append(&label_box);
    row.append(&tile);
    Tile { icon, label }
}

fn show(tile: &(gtk4::Label, gtk4::Label), active: bool, busy: &str, idle: &str) {
    text::set_color(&tile.0, if active { "colError" } else { "colSubtext" });
    tile.1.set_text(if active { busy } else { idle });
}

fn keyword_field(
    page: &Rc<Page>,
    parent: &gtk4::Box,
    placeholder: &str,
    pointer: &'static str,
    list: fn(&Config) -> &Vec<String>,
) {
    let field = page.text_field(
        parent,
        Style::Outlined,
        placeholder,
        move || list(&config::current()).join(", "),
        move |text| {
            let entries: Vec<Value> = text
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(Value::from)
                .collect();
            config::store_value(pointer, Value::Array(entries));
        },
    );
    page.refresh_text_on(pointer, &field);
}
