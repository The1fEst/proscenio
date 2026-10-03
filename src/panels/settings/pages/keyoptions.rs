use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::keyboard::{OPTIONS, option_combo};
use crate::platform::hyprconfig::Area;
use crate::platform::xkbregistry::{self, Catalogue};
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text;

const LABEL_START: i32 = 2;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Keyboard, &OPTIONS);
    let catalogue: Rc<RefCell<Catalogue>> = Rc::default();
    let sources = (&options, &catalogue);

    let special = page.section("emoji_symbols", &tr("Special Character Entry"));
    let ways = text::styled(&tr("Ways of typing symbols and letter variants"));
    text::set_color(&ways, "colSubtext");
    let ways = Centred::new(&ways);
    ways.set_halign(gtk4::Align::Start);
    ways.set_margin_start(LABEL_START);
    special.append(&ways);
    let shown = [
        option_combo(
            &page,
            &special,
            sources,
            ("Alternate characters key", "keyboard_option_key"),
            "lv3",
            "None",
        ),
        option_combo(
            &page,
            &special,
            sources,
            ("Compose key", "text_select_start"),
            "Compose key",
            "None",
        ),
    ];

    let modifiers = page.section("keyboard_command_key", &tr("Modifier Keys"));
    let shown: Vec<Rc<dyn Fn()>> = shown
        .into_iter()
        .chain([
            option_combo(
                &page,
                &modifiers,
                sources,
                ("Caps Lock", "keyboard_capslock"),
                "caps",
                "Default",
            ),
            option_combo(
                &page,
                &modifiers,
                sources,
                ("Ctrl", "keyboard_control_key"),
                "ctrl",
                "Default",
            ),
            option_combo(
                &page,
                &modifiers,
                sources,
                ("Alt and Super", "keyboard_option_key"),
                "altwin",
                "Default",
            ),
        ])
        .collect();

    let follow = Rc::new(move || {
        for show in &shown {
            show();
        }
    });
    follow();
    options.connect_changed({
        let follow = follow.clone();
        move || follow()
    });
    glib::spawn_future_local({
        let catalogue = Rc::downgrade(&catalogue);
        async move {
            let Ok(loaded) = gio::spawn_blocking(xkbregistry::load).await else {
                return;
            };
            if let Some(catalogue) = catalogue.upgrade() {
                catalogue.replace(loaded);
                follow();
            }
        }
    });

    page.keep(catalogue);
    page.keep(options);
    page
}
