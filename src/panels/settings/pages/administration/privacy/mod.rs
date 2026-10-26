pub mod capture;
pub mod lock;

use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;

const TILE_HEIGHT: i32 = 52;
const TILE_SPACING: i32 = 10;

pub struct Tile {
    pub label: gtk4::Label,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let system_section = page.section("tune", &tr("System"));
    page.link_row(
        &system_section,
        "lock",
        &tr("Screen Lock"),
        &tr("What the lock screen looks like and what it lets through"),
        context.subpage_opener("lock"),
    );
    page.link_row(
        &system_section,
        "screenshot_frame_2",
        &tr("Screenshots & Recording"),
        &tr("Where captures go and how the region selector behaves"),
        context.subpage_opener("capture"),
    );

    let safety_section = page.section("work_alert", &tr("Work safety"));
    page.config_switch(
        &safety_section,
        "assignment",
        &tr("Hide clipboard images copied from sussy sources"),
        "/workSafety/enable/clipboard",
        false,
    );
    page.config_switch(
        &safety_section,
        "wallpaper",
        &tr("Hide sussy/anime wallpapers"),
        "/workSafety/enable/wallpaper",
        false,
    );
    let keywords_section = page.subsection(
        &safety_section,
        &tr("Trigger keywords"),
        &tr("Comma-separated. Work safety kicks in when one of these shows up."),
    );
    keyword_field(
        &page,
        &keywords_section,
        &tr("Network names"),
        "/workSafety/triggerCondition/networkNameKeywords",
        |config| &config.safety_networks,
    );
    keyword_field(
        &page,
        &keywords_section,
        &tr("File names"),
        "/workSafety/triggerCondition/fileKeywords",
        |config| &config.safety_files,
    );
    keyword_field(
        &page,
        &keywords_section,
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
    let icon_box = Centered::integral(&icon);
    tile.append(&icon_box);
    let label = text::styled(caption);
    text::set_color(&label, "colOnLayer2");
    label.set_xalign(0.0);
    let label_box = Centered::filling_width(&label);
    label_box.set_hexpand(true);
    tile.append(&label_box);
    row.append(&tile);
    Tile { label }
}

fn keyword_field(
    page: &Rc<Page>,
    parent: &gtk4::Box,
    placeholder: &str,
    pointer: &'static str,
    read_list: fn(&Config) -> &Vec<String>,
) {
    let field = page.text_field(
        parent,
        Style::Outlined,
        placeholder,
        move || read_list(&config::current()).join(", "),
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
