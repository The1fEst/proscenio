use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::platform::security::pam;
use crate::services::network::wifi::Wifi;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::textfield::TextField;

const LABEL_START: i32 = 2;
const BUTTON_TOP: i32 = 4;
const STATUS_START: i32 = 8;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let wifi = Wifi::new();

    let section = page.section("", "");
    let explanation = text::styled(&tr(
        "A hidden network does not announce itself, so it has to be named in full",
    ));
    text::set_color(&explanation, "colSubtext");
    explanation.set_xalign(0.0);
    explanation.set_margin_start(LABEL_START);
    section.append(&explanation);

    let name_group = page.subsection(&section, &tr("Network name"), "");
    let name_field = TextField::new(&page.theme, Style::Outlined, &tr("Name of the network"));
    name_field.root.set_hexpand(true);
    name_group.append(&name_field.root);
    page.keep(name_field.clone());

    let password_group = page.subsection(
        &section,
        &tr("Password"),
        &tr("Leave empty for a network without one"),
    );
    let password_field = page.secret_field(&password_group, &tr("Password"));

    let join: Rc<dyn Fn()> = {
        let wifi = Rc::downgrade(&wifi);
        let (name_field, password_field) =
            (Rc::downgrade(&name_field), Rc::downgrade(&password_field));
        Rc::new(move || {
            let (Some(wifi), Some(name_field), Some(password_field)) = (
                wifi.upgrade(),
                name_field.upgrade(),
                password_field.upgrade(),
            ) else {
                return;
            };
            let ssid = name_field.text().trim().to_owned();
            if ssid.is_empty() {
                return;
            }
            let secret = password_field.text();
            wifi.connect_hidden(&ssid, &secret);
            pam::wipe(&mut secret.into_bytes());
            password_field.set_text("");
        })
    };

    let (connect_button, connect_label) = page.icon_button("wifi_add", true, &tr("Connect"), {
        let join = join.clone();
        move || join()
    });
    connect_button.set_margin_top(BUTTON_TOP);
    section.append(&connect_button);

    let status = text::styled("");
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_margin_start(STATUS_START);
    status.set_visible(false);
    section.append(&status);

    for field in [&name_field, &password_field] {
        let join = join.clone();
        field.connect_accepted(move || join());
    }

    let show_state = {
        let wifi = Rc::downgrade(&wifi);
        let name_field = Rc::downgrade(&name_field);
        move || {
            let (Some(wifi), Some(name_field)) = (wifi.upgrade(), name_field.upgrade()) else {
                return;
            };
            let state = wifi.state.borrow();
            let is_connecting = state.is_hidden_connecting;
            connect_button.set_sensitive(!name_field.text().trim().is_empty() && !is_connecting);
            connect_label.set_text(&if is_connecting {
                tr("Connecting…")
            } else {
                tr("Connect")
            });
            match (&state.hidden_status, is_connecting) {
                (Some(outcome), false) => {
                    status.set_visible(true);
                    match outcome {
                        Ok(()) => {
                            status.set_text(&tr("Connected"));
                            text::set_color(&status, "colSubtext");
                        }
                        Err(message) => {
                            status.set_text(message);
                            text::set_color(&status, "colError");
                        }
                    }
                }
                _ => status.set_visible(false),
            }
        }
    };
    let show_state = Rc::new(show_state);
    show_state();
    name_field.connect_changed({
        let show_state = show_state.clone();
        move || show_state()
    });
    wifi.connect_changed(move || show_state());
    page.keep(wifi);
    page
}
