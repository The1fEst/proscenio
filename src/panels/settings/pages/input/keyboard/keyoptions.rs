use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::input::keyboard::{OPTIONS, option_combo};
use crate::platform::hyprland::hyprconfig::Area;
use crate::platform::system::xkbregistry::{self, Catalog};
use crate::services::hyprland::hyproptions::HyprOptions;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;

const LABEL_START: i32 = 2;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Keyboard, &OPTIONS);
    let catalog: Rc<RefCell<Catalog>> = Rc::default();
    let sources = (&options, &catalog);

    let special_section = page.section("emoji_symbols", &tr("Special Character Entry"));
    let ways_label = text::styled(&tr("Ways of typing symbols and letter variants"));
    text::set_color(&ways_label, "colSubtext");
    let ways_label = Centered::new(&ways_label);
    ways_label.set_halign(gtk4::Align::Start);
    ways_label.set_margin_start(LABEL_START);
    special_section.append(&ways_label);
    let show_special_combos = [
        option_combo(
            &page,
            &special_section,
            sources,
            ("Alternate characters key", "keyboard_option_key"),
            "lv3",
            "None",
        ),
        option_combo(
            &page,
            &special_section,
            sources,
            ("Compose key", "text_select_start"),
            "Compose key",
            "None",
        ),
    ];

    let modifiers_section = page.section("keyboard_command_key", &tr("Modifier Keys"));
    let show_combos: Vec<Rc<dyn Fn()>> = show_special_combos
        .into_iter()
        .chain([
            option_combo(
                &page,
                &modifiers_section,
                sources,
                ("Caps Lock", "keyboard_capslock"),
                "caps",
                "Default",
            ),
            option_combo(
                &page,
                &modifiers_section,
                sources,
                ("Ctrl", "keyboard_control_key"),
                "ctrl",
                "Default",
            ),
            option_combo(
                &page,
                &modifiers_section,
                sources,
                ("Alt and Super", "keyboard_option_key"),
                "altwin",
                "Default",
            ),
        ])
        .collect();

    let show_all = Rc::new(move || {
        for show in &show_combos {
            show();
        }
    });
    show_all();
    options.connect_changed({
        let show_all = show_all.clone();
        move || show_all()
    });
    glib::spawn_future_local({
        let catalog = Rc::downgrade(&catalog);
        async move {
            let Ok(new_catalog) = gio::spawn_blocking(xkbregistry::load).await else {
                return;
            };
            if let Some(catalog) = catalog.upgrade() {
                catalog.replace(new_catalog);
                show_all();
            }
        }
    });

    page.keep(catalog);
    page.keep(options);
    page
}
