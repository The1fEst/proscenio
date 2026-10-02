use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{Context, Page, Style};
use crate::panels::settings::pages::apps::app_dialog;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

const NOTE_START: i32 = 8;
const SHOWN: usize = 40;
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
    all: Vec<FileType>,
    list: gtk4::Box,
    more: gtk4::Label,
    query: RefCell<String>,
    rows: RefCell<Vec<Box<dyn Any>>>,
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
        let found: Vec<&FileType> = if query.trim().is_empty() {
            Vec::new()
        } else {
            self.all
                .iter()
                .filter(|kind| matches(&kind.words, &query))
                .collect()
        };
        let mut rows: Vec<Box<dyn Any>> = Vec::new();
        for kind in found.iter().take(SHOWN) {
            rows.push(Box::new(self.row(&page, kind)));
        }
        let message = if query.trim().is_empty() {
            trf(
                "%1 file types are known. Type a name, an extension or a type to find one",
                &[&self.all.len().to_string()],
            )
        } else if found.is_empty() {
            tr("No file type matches")
        } else if found.len() > SHOWN {
            trf(
                "%1 more match, type more to narrow them down",
                &[&(found.len() - SHOWN).to_string()],
            )
        } else {
            String::new()
        };
        self.more.set_text(&message);
        self.more.set_visible(!message.is_empty());
        self.rows.replace(rows);
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
        let name = text::styled(&kind.description);
        text::set_color(&name, "colOnLayer2");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centred::filling_width(&name));
        let mime = text::styled_sized(&kind.mime, pixel_size::SMALLER);
        text::set_color(&mime, "colSubtext");
        mime.set_xalign(0.0);
        mime.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centred::filling_width(&mime));
        let lines = Centred::filling_width(&lines);
        lines.set_hexpand(true);
        row.append(&lines);
        let opener = text::styled_sized(&default_name(&kind.mime), pixel_size::SMALLER);
        text::set_color(&opener, "colOnLayer2");
        opener.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        opener.set_max_width_chars(24);
        row.append(&Centred::new(&opener));
        let chevron = text::symbol("chevron_right", pixel_size::LARGER as f64);
        text::set_color(&chevron, "colSubtext");
        row.append(&Centred::integral(&chevron));
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
    current: bool,
    picked: Rc<dyn Fn(&gio::AppInfo)>,
) -> RippleButton {
    let item = windowdialog::list_item(theme, current);
    let row = Row::new(ROW_SPACING);
    if let Some(icon) = app.icon() {
        let image = gtk4::Image::from_gicon(&icon);
        image.set_pixel_size(ICON);
        row.append(&image);
    }
    let name = text::styled(&app.display_name());
    text::set_color(&name, if current { "colPrimary" } else { "colOnLayer3" });
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let name = Centred::filling_width(&name);
    name.set_hexpand(true);
    row.append(&name);
    if current {
        let check = text::symbol("check", pixel_size::LARGER as f64);
        text::set_color(&check, "colPrimary");
        row.append(&Centred::integral(&check));
    }
    item.set_content(&row, windowdialog::PADDING as i32, DIALOG_ITEM_VERTICAL);
    let app = app.clone();
    item.connect_clicked(move |_| picked(&app));
    item
}

fn opener_dialog(
    theme: &SharedTheme,
    kind: &FileType,
    present: Present,
    changed: impl Fn() + 'static,
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
    let changed: Rc<dyn Fn()> = Rc::new(changed);
    let current = gio::AppInfo::default_for_type(&kind.mime, false)
        .and_then(|app| app.id())
        .map(|id| id.to_string());
    let picked: Rc<dyn Fn(&gio::AppInfo)> = Rc::new({
        let dialog = Rc::downgrade(&dialog);
        let changed = changed.clone();
        let mime = kind.mime.clone();
        move |app| {
            let _ = app.set_as_default_for_type(&mime);
            changed();
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    let recommended = gio::AppInfo::recommended_for_type(&kind.mime);
    let others: Vec<gio::AppInfo> = gio::AppInfo::fallback_for_type(&kind.mime)
        .into_iter()
        .filter(|app| !recommended.iter().any(|known| known.id() == app.id()))
        .collect();
    for (heading, apps) in [
        (tr("Recommended"), recommended),
        (tr("Also opens it"), others),
    ] {
        if apps.is_empty() {
            continue;
        }
        let label = windowdialog::section_header(&heading);
        label.set_margin_start(windowdialog::PADDING as i32);
        label.set_margin_top(DIALOG_ITEM_VERTICAL);
        list.append(&label);
        for app in apps {
            let is_current = app.id().map(|id| id.to_string()) == current;
            list.append(&app_item(theme, &app, is_current, picked.clone()));
        }
    }
    if list.first_child().is_none() {
        let empty = text::styled(&tr("No installed application says it opens this type"));
        text::set_color(&empty, "colSubtext");
        empty.set_xalign(0.0);
        empty.set_margin_start(windowdialog::PADDING as i32);
        list.append(&empty);
    }
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_child(Some(&list));
    crate::ui::widgets::flickable::follow_scroll_settings(&scroll);
    dialog.column.add(
        &scroll,
        Place {
            left: -windowdialog::PADDING,
            right: -windowdialog::PADDING,
            fill_width: true,
            fill_height: true,
            ..Place::default()
        },
    );
    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());
    let (buttons, place) = windowdialog::button_row();
    let reset = windowdialog::button(theme, &tr("Reset"));
    reset.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        let mime = kind.mime.clone();
        let changed = changed.clone();
        move |_| {
            let _ = gio::AppInfo::reset_type_associations(&mime);
            changed();
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    buttons.append(&reset);
    buttons.append(&windowdialog::spacer());
    let other = windowdialog::button(theme, &tr("Other application…"));
    other.connect_clicked({
        let theme = theme.clone();
        let mime = kind.mime.clone();
        let changed = changed.clone();
        move |_| {
            let mime = mime.clone();
            let changed = changed.clone();
            let chooser = app_dialog(&theme, &tr("Select an application"), move |id| {
                let found = gio::AppInfo::all()
                    .into_iter()
                    .find(|app| app.id().as_deref() == Some(id.as_str()));
                if let Some(app) = found {
                    let _ = app.set_as_default_for_type(&mime);
                }
                changed();
            });
            present(chooser);
        }
    });
    buttons.append(&other);
    let cancel = windowdialog::button(theme, &tr("Cancel"));
    cancel.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    buttons.append(&cancel);
    dialog.column.add(&buttons, place);
    dialog
}

fn all_types() -> Vec<FileType> {
    let mut found: Vec<FileType> = gio::content_types_get_registered()
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
    found.sort_by(|one, other| {
        one.description
            .to_lowercase()
            .cmp(&other.description.to_lowercase())
    });
    found
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let section = page.section("", "");
    let search = TextField::new(&page.theme, Style::Outlined, &tr("Find a file type"));
    search.root.set_hexpand(true);
    section.append(&search.root);
    let more = text::styled("");
    text::set_color(&more, "colSubtext");
    more.set_xalign(0.0);
    more.set_wrap(true);
    more.set_margin_start(NOTE_START);
    section.append(&more);
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    section.append(&list);
    let types = Rc::new(Types {
        page: Rc::downgrade(&page),
        present: Rc::new(context.dialog_presenter()),
        all: all_types(),
        list,
        more,
        query: RefCell::new(String::new()),
        rows: RefCell::new(Vec::new()),
    });
    search.connect_changed({
        let types = Rc::downgrade(&types);
        let search = Rc::downgrade(&search);
        move || {
            let (Some(types), Some(search)) = (types.upgrade(), search.upgrade()) else {
                return;
            };
            types.query.replace(search.text());
            types.fill();
        }
    });
    types.fill();
    glib::idle_add_local_once({
        let search = Rc::downgrade(&search);
        move || {
            if let Some(search) = search.upgrade() {
                search.grab_focus();
            }
        }
    });
    page.keep(search);
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
