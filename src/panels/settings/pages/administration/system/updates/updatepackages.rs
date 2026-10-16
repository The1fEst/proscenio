use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{Context, Page};
use crate::services::updates::Updates;
use crate::services::updates::packages::version_change;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

const ROW_HEIGHT: i32 = 56;
const ROW_START: i32 = 12;
const ROW_END: i32 = 12;
const ROW_SPACING: i32 = 12;
const VERSION_SPACING: i32 = 4;
const COPY_SIZE: i32 = 32;
const COPY_ICON: f64 = 20.0;
const COPY_RESET: Duration = Duration::from_millis(1500);

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let updates = context.services.updates.clone();

    let empty_section = page.section("", "");
    let empty_label = up_to_date_label();
    empty_section.append(&empty_label);
    let repository_section = page.section("deployed_code", &tr("Repositories"));
    let aur_section = page.section("deployed_code_account", &tr("AUR"));

    let show_packages = {
        let updates = updates.clone();
        let theme = context.theme.clone();
        move || {
            let repository_packages = updates.repository_packages.borrow();
            let aur_packages = updates.aur_packages.borrow();
            let build_row = |line: &String| package_row(&theme, line);
            fill(&repository_section, &repository_packages, build_row);
            fill(&aur_section, &aur_packages, build_row);
            let has_check_failed = updates.has_package_check_failed.get();
            empty_label.set_text(&if has_check_failed {
                tr("Could not check for updates")
            } else {
                tr("Up to date")
            });
            if let Some(section) = empty_section.parent() {
                section.set_visible(
                    has_check_failed || (repository_packages.is_empty() && aur_packages.is_empty()),
                );
            }
        }
    };
    show_packages();
    page.keep(updates.subscribe(show_packages));
    page
}

pub fn summary(updates: &Updates) -> String {
    trf(
        "%1 from the repositories, %2 from AUR",
        &[
            &updates.repository_packages.borrow().len().to_string(),
            &updates.aur_packages.borrow().len().to_string(),
        ],
    )
}

pub fn fill<T>(content: &gtk4::Box, items: &[T], build_row: impl Fn(&T) -> gtk4::Box) {
    if let Some(section) = content.parent() {
        section.set_visible(!items.is_empty());
    }
    while let Some(child) = content.last_child() {
        content.remove(&child);
    }
    for item in items {
        content.append(&build_row(item));
    }
}

pub fn copy_button(theme: &SharedTheme, copied_text: &str) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_radius(COPY_SIZE as f64 / 2.0);
    button.set_size_request(COPY_SIZE, COPY_SIZE);
    button.set_valign(gtk4::Align::Center);
    button.set_margin_end(ROW_END);
    let symbol = text::symbol("content_copy", COPY_ICON);
    text::set_color(&symbol, "colOnLayer2");
    button.set_content(&Centered::integral(&symbol), 0, 0);
    let copied_text = copied_text.to_owned();
    button.connect_clicked(move |button| {
        button.display().clipboard().set_text(&copied_text);
        symbol.set_text("check");
        let symbol = symbol.clone();
        glib::timeout_add_local_once(COPY_RESET, move || symbol.set_text("content_copy"));
    });
    button
}

pub fn up_to_date_label() -> gtk4::Label {
    let label = text::styled(&tr("Up to date"));
    text::set_color(&label, "colSubtext");
    label.set_xalign(0.0);
    label
}

pub fn card(icon: &str, title: &str, is_title_selectable: bool) -> (gtk4::Box, gtk4::Box) {
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("settings-row-card");
    card.set_size_request(-1, ROW_HEIGHT);
    let content = Row::new(ROW_SPACING);
    content.set_margin_start(ROW_START);
    content.set_margin_end(ROW_END);
    content.set_hexpand(true);

    let symbol = text::symbol(icon, pixel_size::LARGER as f64);
    text::set_color(&symbol, "colOnSecondaryContainer");
    content.append(&Centered::integral(&symbol));

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_hexpand(true);
    lines.set_valign(gtk4::Align::Center);
    let title_label = text::styled(title);
    text::set_color(&title_label, "colOnLayer2");
    title_label.set_xalign(0.0);
    title_label.set_selectable(is_title_selectable);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centered::filling_width(&title_label));
    content.append(&lines);
    card.append(&content);
    (card, lines)
}

fn package_row(theme: &SharedTheme, line: &str) -> gtk4::Box {
    let (name, from, to) = version_change(line).unwrap_or((line, "", ""));
    let (card, lines) = card("deployed_code", name, true);
    let versions = gtk4::Box::new(gtk4::Orientation::Horizontal, VERSION_SPACING);
    for (part, color) in [
        (from, "colSubtext"),
        ("→", "colSubtext"),
        (to, "colPrimary"),
    ] {
        let label = text::styled_sized(part, pixel_size::SMALLER);
        text::set_color(&label, color);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        versions.append(&label);
    }
    versions.set_visible(!to.is_empty());
    lines.append(&versions);
    card.append(&copy_button(theme, name));
    card
}
