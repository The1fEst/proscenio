pub mod about;
pub mod advanced;
pub mod autostart;
pub mod datetime;
pub mod region;
pub mod services;
pub mod updates;
pub mod users;

use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::services::session::accounts;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let main_section = page.section("", "");
    page.link_row(
        &main_section,
        "language",
        &tr("Region & Language"),
        &tr("What language the interface is in"),
        context.subpage_opener("region"),
    );
    page.link_row(
        &main_section,
        "nest_clock_farsight_analog",
        &tr("Date & Time"),
        &tr("Clock format, date formats and the pomodoro timer"),
        context.subpage_opener("datetime"),
    );
    let user_name_label = page.link_row(
        &main_section,
        "person",
        &tr("Users"),
        "",
        context.subpage_opener("users"),
    );
    let user_name_label = user_name_label.downgrade();
    accounts::read(move |account| {
        if let Some(user_name_label) = user_name_label.upgrade() {
            user_name_label.set_text(account.display_name());
            user_name_label.set_visible(!account.display_name().is_empty());
        }
    });
    page.link_row(
        &main_section,
        "start",
        &tr("Autostart"),
        &tr("Apps and commands that start with the session"),
        context.subpage_opener("autostart"),
    );
    page.link_row(
        &main_section,
        "deployed_code_update",
        &tr("Updates"),
        &tr("Install package and shell updates"),
        context.subpage_opener("updates"),
    );
    page.link_row(
        &main_section,
        "info",
        &tr("About"),
        &tr("What this machine is and what it runs"),
        context.subpage_opener("about"),
    );

    let shell_section = page.section("tune", &tr("The shell itself"));
    page.link_row(
        &shell_section,
        "settings",
        &tr("Services"),
        &tr("Weather, updates, resources and the conflict killer"),
        context.subpage_opener("services"),
    );
    page.link_row(
        &shell_section,
        "construction",
        &tr("Advanced"),
        &tr("Workarounds and settings that can break things"),
        context.subpage_opener("advanced"),
    );
    page
}
