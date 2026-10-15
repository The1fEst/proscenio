use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Parent};
use crate::panels::settings::pages::desktop::appearance::follow;
use crate::platform::system::appearance::Parts;
use crate::services::session::appearance::DesktopAppearance;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::spinbox::SpinBox;
use crate::ui::widgets::layout::centered::Centered;

const FONT_LABEL_WIDTH: i32 = 110;
const FONT_LABEL_START: i32 = 2;
const FONT_SIZES: (i64, i64) = (5, 72);
const FONT_ROLES: [(&str, &[&str], &str); 6] = [
    ("general", &["main", "reading"], "General"),
    ("fixed", &["monospace"], "Fixed width"),
    ("title", &["title"], "Titles"),
    ("small", &[], "Small"),
    ("toolbar", &[], "Toolbar"),
    ("menu", &[], "Menu"),
];
const SHELL_FONTS: [(&str, &str, &str); 2] = [
    (
        "/appearance/fonts/iconNerd",
        "JetBrains Mono NF",
        "Nerd icons",
    ),
    (
        "/appearance/fonts/expressive",
        "Space Grotesk",
        "Expressive",
    ),
];
const BULK_SHELL_KEYS: [&str; 3] = ["main", "reading", "title"];

pub fn family_options(current: &str, families: Vec<String>) -> Vec<String> {
    if current.is_empty() || families.iter().any(|family| family == current) {
        return families;
    }
    std::iter::once(current.to_owned())
        .chain(families)
        .collect()
}

fn store_shell_fonts(keys: &[&str], family: &str) {
    for key in keys {
        config::store_value(&format!("/appearance/fonts/{key}"), Value::from(family));
    }
}

fn font_label(parent: &impl Parent, name: &str) {
    let label = text::styled(name);
    text::set_color(&label, "colSubtext");
    label.set_xalign(0.0);
    let label_box = Centered::filling_width(&label);
    label_box.set_size_request(FONT_LABEL_WIDTH, -1);
    label_box.set_margin_start(FONT_LABEL_START);
    parent.add(&label_box);
}

fn size_spin(page: &Page, parent: &impl Parent) -> (gtk4::Box, Rc<SpinBox>) {
    let spin = SpinBox::new(&page.theme, FONT_SIZES.0, FONT_SIZES.1, 1, 0);
    let row = page.spin_row(parent, "", "", &spin);
    (row, spin)
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let appearance = DesktopAppearance::new();

    let fonts_section = page.section("", "");
    let roles_section = page.subsection(
        &fonts_section,
        &tr("Apps & panels"),
        &tr("Sizes apply to GTK and Qt apps; panels scale their own."),
    );
    for (role, shell_keys, name) in FONT_ROLES {
        let row = page.row(&roles_section);
        font_label(&row, &tr(name));
        let combo = page.combo(&row, "font_download");
        let (_, size_box) = size_spin(&page, &row);
        follow(&page, &appearance, {
            let combo = Rc::downgrade(&combo);
            let size_box = Rc::downgrade(&size_box);
            move |appearance| {
                let (Some(combo), Some(size_box)) = (combo.upgrade(), size_box.upgrade()) else {
                    return;
                };
                let font = appearance.font(role);
                combo.set_items_showing(
                    &family_options(&font.family, appearance.family_names()),
                    &font.family,
                );
                size_box.set_value(font.size);
            }
        });
        combo.connect_activated({
            let appearance = Rc::downgrade(&appearance);
            let combo = Rc::downgrade(&combo);
            move |index| {
                let (Some(appearance), Some(combo)) = (appearance.upgrade(), combo.upgrade())
                else {
                    return;
                };
                let Some(family) = combo.item(index) else {
                    return;
                };
                appearance.set_font(
                    role,
                    Parts {
                        family: Some(family.clone()),
                        ..Parts::default()
                    },
                );
                store_shell_fonts(shell_keys, &family);
            }
        });
        size_box.connect_changed({
            let appearance = Rc::downgrade(&appearance);
            move |value| {
                if let Some(appearance) = appearance.upgrade() {
                    appearance.set_font(
                        role,
                        Parts {
                            size: Some(value),
                            ..Parts::default()
                        },
                    );
                }
            }
        });
    }

    let bulk_section = page.subsection(
        &fonts_section,
        &tr("Adjust all"),
        &tr("Sets every role above at once. Fixed width keeps its own family so code stays monospaced."),
    );
    let should_change_family = Rc::new(Cell::new(true));
    let should_change_size = Rc::new(Cell::new(false));
    let bulk_family: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let bulk_size: Rc<Cell<Option<i64>>> = Rc::new(Cell::new(None));
    let read_bulk_family = {
        let bulk_family = bulk_family.clone();
        move |appearance: &DesktopAppearance| {
            bulk_family
                .borrow()
                .clone()
                .unwrap_or_else(|| appearance.font("general").family)
        }
    };
    let read_bulk_size = {
        let bulk_size = bulk_size.clone();
        move |appearance: &DesktopAppearance| {
            bulk_size
                .get()
                .unwrap_or_else(|| appearance.font("general").size)
        }
    };
    let targets_row = page.uniform_row(&bulk_section);
    let family_switch = page.switch(&targets_row, "font_download", &tr("Family"), {
        let should_change_family = should_change_family.clone();
        move |is_on| should_change_family.set(is_on)
    });
    family_switch.bind({
        let should_change_family = should_change_family.clone();
        move || should_change_family.get()
    });
    let size_switch = page.switch(&targets_row, "format_size", &tr("Size"), {
        let should_change_size = should_change_size.clone();
        move |is_on| should_change_size.set(is_on)
    });
    size_switch.bind({
        let should_change_size = should_change_size.clone();
        move || should_change_size.get()
    });
    let values_row = page.row(&bulk_section);
    let family_combo = page.combo(&values_row, "font_download");
    let (size_row, size_box) = size_spin(&page, &values_row);
    let refresh_bulk = {
        let family_combo = Rc::downgrade(&family_combo);
        let size_box = Rc::downgrade(&size_box);
        let size_row = size_row.clone();
        let should_change_family = should_change_family.clone();
        let should_change_size = should_change_size.clone();
        let read_bulk_family = read_bulk_family.clone();
        let read_bulk_size = read_bulk_size.clone();
        move |appearance: &DesktopAppearance| {
            let (Some(family_combo), Some(size_box)) = (family_combo.upgrade(), size_box.upgrade())
            else {
                return;
            };
            let family = read_bulk_family(appearance);
            family_combo
                .set_items_showing(&family_options(&family, appearance.family_names()), &family);
            family_combo.set_enabled(should_change_family.get());
            size_box.set_value(read_bulk_size(appearance));
            Page::set_spin_row_enabled(&size_row, &size_box, should_change_size.get());
        }
    };
    let refresh_bulk = Rc::new(refresh_bulk);
    follow(&page, &appearance, {
        let refresh_bulk = refresh_bulk.clone();
        move |appearance| refresh_bulk(appearance)
    });
    for switch in [&family_switch, &size_switch] {
        switch.button.connect_clicked({
            let appearance = Rc::downgrade(&appearance);
            let refresh_bulk = refresh_bulk.clone();
            move |_| {
                if let Some(appearance) = appearance.upgrade() {
                    refresh_bulk(&appearance);
                }
            }
        });
    }
    family_combo.connect_activated({
        let appearance = Rc::downgrade(&appearance);
        let family_combo = Rc::downgrade(&family_combo);
        let bulk_family = bulk_family.clone();
        let refresh_bulk = refresh_bulk.clone();
        move |index| {
            let (Some(appearance), Some(family_combo)) =
                (appearance.upgrade(), family_combo.upgrade())
            else {
                return;
            };
            bulk_family.replace(family_combo.item(index));
            refresh_bulk(&appearance);
        }
    });
    size_box.connect_changed({
        let bulk_size = bulk_size.clone();
        move |value| bulk_size.set(Some(value))
    });
    let (apply_button, _) = page.icon_button("done_all", true, &tr("Apply to all fonts"), {
        let appearance = Rc::downgrade(&appearance);
        move || {
            let Some(appearance) = appearance.upgrade() else {
                return;
            };
            let family = read_bulk_family(&appearance);
            let parts = Parts {
                family: (should_change_family.get() && !family.is_empty()).then_some(family),
                size: should_change_size
                    .get()
                    .then(|| read_bulk_size(&appearance)),
                ..Parts::default()
            };
            let family = parts.family.clone();
            appearance.set_font("all", parts);
            if let Some(family) = family {
                store_shell_fonts(&BULK_SHELL_KEYS, &family);
            }
        }
    });
    bulk_section.append(&apply_button);

    let shell_only_section = page.subsection(
        &fonts_section,
        &tr("Panels only"),
        &tr("Faces the shell uses that GTK and Qt have no equivalent for"),
    );
    for (pointer, default, name) in SHELL_FONTS {
        let row = page.row(&shell_only_section);
        font_label(&row, &tr(name));
        let combo = page.combo(&row, "font_download");
        let read_family = move || config::value_str(pointer).unwrap_or_else(|| default.to_owned());
        let update_combo = Rc::new({
            let combo = Rc::downgrade(&combo);
            let appearance = Rc::downgrade(&appearance);
            move || {
                let (Some(combo), Some(appearance)) = (combo.upgrade(), appearance.upgrade())
                else {
                    return;
                };
                let family = read_family();
                combo.set_items_showing(
                    &family_options(&family, appearance.family_names()),
                    &family,
                );
            }
        });
        update_combo();
        page.keep(appearance.watch({
            let update_combo = update_combo.clone();
            move || update_combo()
        }));
        page.watch(pointer, move || update_combo());
        combo.connect_activated({
            let combo = Rc::downgrade(&combo);
            move |index| {
                if let Some(family) = combo.upgrade().and_then(|combo| combo.item(index)) {
                    config::store_value(pointer, Value::from(family));
                }
            }
        });
    }

    page.keep(appearance);
    page
}
