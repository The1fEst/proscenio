use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::services::wifi::Wifi;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;

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
    let name = TextField::new(&page.theme, Style::Outlined, &tr("Name of the network"));
    name.root.set_hexpand(true);
    name_group.append(&name.root);
    page.keep(name.clone());

    let password_group = page.subsection(
        &section,
        &tr("Password"),
        &tr("Leave empty for a network without one"),
    );
    let password = page.secret_field(&password_group, &tr("Password"));

    let join: Rc<dyn Fn()> = {
        let wifi = Rc::downgrade(&wifi);
        let (name, password) = (Rc::downgrade(&name), Rc::downgrade(&password));
        Rc::new(move || {
            let (Some(wifi), Some(name), Some(password)) =
                (wifi.upgrade(), name.upgrade(), password.upgrade())
            else {
                return;
            };
            let ssid = name.text().trim().to_owned();
            if ssid.is_empty() {
                return;
            }
            wifi.connect_hidden(&ssid, &password.text());
            password.set_text("");
        })
    };

    let (connect, connect_label) = page.icon_button("wifi_add", true, &tr("Connect"), {
        let join = join.clone();
        move || join()
    });
    connect.set_margin_top(BUTTON_TOP);
    section.append(&connect);

    let status = text::styled("");
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_margin_start(STATUS_START);
    status.set_visible(false);
    section.append(&status);

    for field in [&name, &password] {
        let join = join.clone();
        field.connect_accepted(move || join());
    }

    let follow = {
        let wifi = Rc::downgrade(&wifi);
        let name = Rc::downgrade(&name);
        move || {
            let (Some(wifi), Some(name)) = (wifi.upgrade(), name.upgrade()) else {
                return;
            };
            let state = wifi.state.borrow();
            let connecting = state.hidden_connecting;
            connect.set_sensitive(!name.text().trim().is_empty() && !connecting);
            connect_label.set_text(&if connecting {
                tr("Connecting…")
            } else {
                tr("Connect")
            });
            match (&state.hidden_status, connecting) {
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
    let follow = Rc::new(follow);
    follow();
    name.connect_changed({
        let follow = follow.clone();
        move || follow()
    });
    wifi.connect_changed(move || follow());
    page.keep(wifi);
    page
}
