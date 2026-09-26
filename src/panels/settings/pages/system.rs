use gtk4::prelude::*;
use std::rc::Rc;

use crate::panels::settings::content::{Context, Page};
use crate::services::accounts;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let main = page.section("", "");
    page.link_row(
        &main,
        "language",
        "Region & Language",
        "What language the interface is in",
        context.subpage_opener("region"),
    );
    page.link_row(
        &main,
        "nest_clock_farsight_analog",
        "Date & Time",
        "Clock format, date formats and the pomodoro timer",
        context.subpage_opener("datetime"),
    );
    let user = page.link_row(
        &main,
        "person",
        "Users",
        "",
        context.subpage_opener("users"),
    );
    let user = user.downgrade();
    accounts::read(move |account| {
        if let Some(user) = user.upgrade() {
            user.set_text(account.display_name());
            user.set_visible(!account.display_name().is_empty());
        }
    });
    page.link_row(
        &main,
        "info",
        "About",
        "What this machine is and what it runs",
        context.subpage_opener("about"),
    );

    let shell = page.section("tune", "The shell itself");
    page.link_row(
        &shell,
        "settings",
        "Services",
        "Weather, updates, resources and the conflict killer",
        context.subpage_opener("services"),
    );
    page.link_row(
        &shell,
        "construction",
        "Advanced",
        "Workarounds and settings that can break things",
        context.subpage_opener("advanced"),
    );
    page
}
