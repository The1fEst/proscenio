use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf, trn};
use crate::panels::settings::content::{Context, Page, Style};
use crate::panels::settings::pages::software::apps::app_dialog;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const NOTE_START: i32 = 8;
const MAX_ROWS: usize = 40;
const ROW_HEIGHT: i32 = 56;
const ROW_PADDING: i32 = 12;
const ROW_SPACING: i32 = 12;
const ICON: i32 = 24;
const DIALOG_WIDTH: f64 = 460.0;
const DIALOG_HEIGHT: f64 = 600.0;
const DIALOG_ITEM_VERTICAL: i32 = 12;

type Present = Rc<dyn Fn(Rc<WindowDialog>)>;

#[derive(Clone)]
struct FileType {
    mime: String,
    description: String,
    words: String,
}

struct Types {
    page: Weak<Page>,
    present: Present,
    file_types: Vec<FileType>,
    list: gtk4::Box,
    more_label: gtk4::Label,
    query: RefCell<String>,
    row_buttons: RefCell<Vec<Box<dyn Any>>>,
}

pub fn matches(words: &str, query: &str) -> bool {
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| words.contains(word))
}

fn default_name(mime: &str) -> String {
    gio::AppInfo::default_for_type(mime, false)
        .map(|app| app.display_name().to_string())
        .unwrap_or_else(|| tr("Nothing opens it"))
}

impl Types {
    fn fill(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let query = self.query.borrow().clone();
        let matching_types: Vec<&FileType> = if query.trim().is_empty() {
            Vec::new()
        } else {
            self.file_types
                .iter()
                .filter(|kind| matches(&kind.words, &query))
                .collect()
        };
        let mut row_buttons: Vec<Box<dyn Any>> = Vec::new();
        for kind in matching_types.iter().take(MAX_ROWS) {
            row_buttons.push(Box::new(self.row(&page, kind)));
        }
        let message = if query.trim().is_empty() {
            trn(
                "%1 file type is known. Type a name, an extension or a type to find it",
                "%1 file types are known. Type a name, an extension or a type to find one",
                self.file_types.len() as u64,
            )
        } else if matching_types.is_empty() {
            tr("No file type matches")
        } else if matching_types.len() > MAX_ROWS {
            trn(
                "%1 more match, type more to narrow it down",
                "%1 more matches, type more to narrow them down",
                (matching_types.len() - MAX_ROWS) as u64,
            )
        } else {
            String::new()
        };
        self.more_label.set_text(&message);
        self.more_label.set_visible(!message.is_empty());
        self.row_buttons.replace(row_buttons);
    }

    fn row(self: &Rc<Self>, page: &Page, kind: &FileType) -> RippleButton {
        let button = RippleButton::new(&page.theme);
        button.set_radius(crate::ui::theme::rounding::SMALL as f64);
        button.set_look(Look {
            background: |theme| theme.colors.col_layer2,
            ..Look::default()
        });
        button.set_size_request(-1, ROW_HEIGHT);
        let row = Row::new(ROW_SPACING);
        let image = gtk4::Image::from_gicon(&gio::content_type_get_icon(&kind.mime));
        image.set_pixel_size(ICON);
        row.append(&image);
        let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        lines.set_hexpand(true);
        lines.set_valign(gtk4::Align::Center);
        let name_label = text::styled(&kind.description);
        text::set_color(&name_label, "colOnLayer2");
        name_label.set_xalign(0.0);
        name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centered::filling_width(&name_label));
        let mime_label = text::styled_sized(&kind.mime, pixel_size::SMALLER);
        text::set_color(&mime_label, "colSubtext");
        mime_label.set_xalign(0.0);
        mime_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centered::filling_width(&mime_label));
        let lines_box = Centered::filling_width(&lines);
        lines_box.set_hexpand(true);
        row.append(&lines_box);
        let opener_label = text::styled_sized(&default_name(&kind.mime), pixel_size::SMALLER);
        text::set_color(&opener_label, "colOnLayer2");
        opener_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        opener_label.set_max_width_chars(24);
        row.append(&Centered::new(&opener_label));
        let chevron = text::symbol("chevron_right", pixel_size::LARGER as f64);
        text::set_color(&chevron, "colSubtext");
        row.append(&Centered::integral(&chevron));
        button.set_content(&row, ROW_PADDING, 0);
        button.connect_clicked({
            let types = Rc::downgrade(self);
            let kind = kind.clone();
            move |_| {
                if let Some(types) = types.upgrade() {
                    types.choose(&kind);
                }
            }
        });
        self.list.append(&button);
        button
    }

    fn choose(self: &Rc<Self>, kind: &FileType) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let dialog = opener_dialog(&page.theme, kind, self.present.clone(), {
            let types = Rc::downgrade(self);
            move || {
                if let Some(types) = types.upgrade() {
                    types.fill();
                }
            }
        });
        (self.present)(dialog);
    }
}

fn app_item(
    theme: &SharedTheme,
    app: &gio::AppInfo,
    is_current: bool,
    on_picked: Rc<dyn Fn(&gio::AppInfo)>,
) -> RippleButton {
    let item = windowdialog::list_item(theme, is_current);
    let row = Row::new(ROW_SPACING);
    if let Some(icon) = app.icon() {
        let image = gtk4::Image::from_gicon(&icon);
        image.set_pixel_size(ICON);
        row.append(&image);
    }
    let name_label = text::styled(&app.display_name());
    text::set_color(
        &name_label,
        if is_current {
            "colPrimary"
        } else {
            "colOnLayer3"
        },
    );
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let name_box = Centered::filling_width(&name_label);
    name_box.set_hexpand(true);
    row.append(&name_box);
    if is_current {
        let check = text::symbol("check", pixel_size::LARGER as f64);
        text::set_color(&check, "colPrimary");
        row.append(&Centered::integral(&check));
    }
    item.set_content(&row, windowdialog::PADDING as i32, DIALOG_ITEM_VERTICAL);
    let app = app.clone();
    item.connect_clicked(move |_| on_picked(&app));
    item
}

fn opener_dialog(
    theme: &SharedTheme,
    kind: &FileType,
    present: Present,
    on_changed: impl Fn() + 'static,
) -> Rc<WindowDialog> {
    let dialog = WindowDialog::new(theme, Some(DIALOG_HEIGHT));
    dialog.set_background_width(DIALOG_WIDTH);
    dialog.column.add(
        &windowdialog::title(&trf("Open %1 with", &[&kind.description])),
        Place::wide(),
    );
    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let on_changed: Rc<dyn Fn()> = Rc::new(on_changed);
    let current_id = gio::AppInfo::default_for_type(&kind.mime, false)
        .and_then(|app| app.id())
        .map(|id| id.to_string());
    let on_picked: Rc<dyn Fn(&gio::AppInfo)> = Rc::new({
        let dialog = Rc::downgrade(&dialog);
        let on_changed = on_changed.clone();
        let mime = kind.mime.clone();
        move |app| {
            let _ = app.set_as_default_for_type(&mime);
            on_changed();
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    let recommended_apps = gio::AppInfo::recommended_for_type(&kind.mime);
    let other_apps: Vec<gio::AppInfo> = gio::AppInfo::fallback_for_type(&kind.mime)
        .into_iter()
        .filter(|app| !recommended_apps.iter().any(|known| known.id() == app.id()))
        .collect();
    for (heading, apps) in [
        (tr("Recommended"), recommended_apps),
        (tr("Also opens it"), other_apps),
    ] {
        if apps.is_empty() {
            continue;
        }
        let heading_label = windowdialog::section_header(&heading);
        heading_label.set_margin_start(windowdialog::PADDING as i32);
        heading_label.set_margin_top(DIALOG_ITEM_VERTICAL);
        list.append(&heading_label);
        for app in apps {
            let is_current = app.id().map(|id| id.to_string()) == current_id;
            list.append(&app_item(theme, &app, is_current, on_picked.clone()));
        }
    }
    if list.first_child().is_none() {
        let empty_label = text::styled(&tr("No installed application says it opens this type"));
        text::set_color(&empty_label, "colSubtext");
        empty_label.set_xalign(0.0);
        empty_label.set_margin_start(windowdialog::PADDING as i32);
        list.append(&empty_label);
    }
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_child(Some(&list));
    crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);
    dialog.column.add(
        &scroll,
        Place {
            left: -windowdialog::PADDING,
            right: -windowdialog::PADDING,
            fills_width: true,
            fills_height: true,
            ..Place::default()
        },
    );
    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());
    let (buttons, place) = windowdialog::button_row();
    let reset_button = windowdialog::button(theme, &tr("Reset"));
    reset_button.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        let mime = kind.mime.clone();
        let on_changed = on_changed.clone();
        move |_| {
            let _ = gio::AppInfo::reset_type_associations(&mime);
            on_changed();
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    buttons.append(&reset_button);
    buttons.append(&windowdialog::spacer());
    let other_button = windowdialog::button(theme, &tr("Other application…"));
    other_button.connect_clicked({
        let theme = theme.clone();
        let mime = kind.mime.clone();
        let on_changed = on_changed.clone();
        move |_| {
            let mime = mime.clone();
            let on_changed = on_changed.clone();
            let chooser = app_dialog(&theme, &tr("Select an application"), move |id| {
                let matching_app = gio::AppInfo::all()
                    .into_iter()
                    .find(|app| app.id().as_deref() == Some(id.as_str()));
                if let Some(app) = matching_app {
                    let _ = app.set_as_default_for_type(&mime);
                }
                on_changed();
            });
            present(chooser);
        }
    });
    buttons.append(&other_button);
    let cancel_button = windowdialog::button(theme, &tr("Cancel"));
    cancel_button.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    buttons.append(&cancel_button);
    dialog.column.add(&buttons, place);
    dialog
}

fn all_types() -> Vec<FileType> {
    let mut types: Vec<FileType> = gio::content_types_get_registered()
        .into_iter()
        .map(|mime| mime.to_string())
        .filter(|mime| mime.contains('/'))
        .map(|mime| {
            let description = gio::content_type_get_description(&mime).to_string();
            let words = format!("{mime}\n{description}").to_lowercase();
            FileType {
                mime,
                description,
                words,
            }
        })
        .collect();
    types.sort_by(|one, other| {
        one.description
            .to_lowercase()
            .cmp(&other.description.to_lowercase())
    });
    types
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let section = page.section("", "");
    let search_field = TextField::new(&page.theme, Style::Outlined, &tr("Find a file type"));
    search_field.root.set_hexpand(true);
    section.append(&search_field.root);
    let more_label = text::styled("");
    text::set_color(&more_label, "colSubtext");
    more_label.set_xalign(0.0);
    more_label.set_wrap(true);
    more_label.set_margin_start(NOTE_START);
    section.append(&more_label);
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    section.append(&list);
    let types = Rc::new(Types {
        page: Rc::downgrade(&page),
        present: Rc::new(context.dialog_presenter()),
        file_types: all_types(),
        list,
        more_label,
        query: RefCell::new(String::new()),
        row_buttons: RefCell::new(Vec::new()),
    });
    search_field.connect_changed({
        let types = Rc::downgrade(&types);
        let search_field = Rc::downgrade(&search_field);
        move || {
            let (Some(types), Some(search_field)) = (types.upgrade(), search_field.upgrade())
            else {
                return;
            };
            types.query.replace(search_field.text());
            types.fill();
        }
    });
    types.fill();
    glib::idle_add_local_once({
        let search_field = Rc::downgrade(&search_field);
        move || {
            if let Some(search_field) = search_field.upgrade() {
                search_field.grab_focus();
            }
        }
    });
    page.keep(search_field);
    page.keep(types);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_typed_word_has_to_match() {
        let words = "image/png\npng image";
        assert!(matches(words, "PNG"));
        assert!(matches(words, "image png"));
        assert!(!matches(words, "png jpeg"));
    }
}
