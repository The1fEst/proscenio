use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::group::{GroupButton, Look as GroupLook};
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::text::{self, Family};
use crate::ui::widgets::tooltip::{self, Tooltip};

const INSET: i32 = 4;
const SPACING: i32 = 8;
const CRUMB_SPACING: i32 = 2;
const CRUMB_PADDING: (f64, f64) = (12.0, 8.0);
const CRUMB_TEXT_HEIGHT: f64 = 15.0;
const BUTTON_PADDING: (i32, i32) = (8, 6);

type Navigate = Rc<dyn Fn(&str)>;

pub struct AddressBar {
    pub widget: gtk4::Box,
    theme: SharedTheme,
    crumbs: gtk4::Box,
    scroller: gtk4::ScrolledWindow,
    input: gtk4::Entry,
    pages: gtk4::Stack,
    edit: RippleButton,
    edit_symbol: gtk4::Label,
    directory: RefCell<String>,
    shown_path: RefCell<String>,
    breadcrumb: Cell<bool>,
    navigate: Navigate,
    _tip: Rc<Tooltip>,
}

impl AddressBar {
    pub fn new(theme: &SharedTheme, navigate: impl Fn(&str) + 'static) -> Rc<Self> {
        let navigate: Navigate = Rc::new(navigate);

        let up_symbol = text::symbol("drive_folder_upload", pixel_size::LARGER as f64);
        text::set_color(&up_symbol, "m3onBackground");
        let up = RippleButton::new(theme);
        up.set_content(
            &Centred::integral(&up_symbol),
            BUTTON_PADDING.0,
            BUTTON_PADDING.1,
        );
        up.set_valign(gtk4::Align::Center);

        let crumbs = gtk4::Box::new(gtk4::Orientation::Horizontal, CRUMB_SPACING);
        crumbs.set_valign(gtk4::Align::Center);
        let scroller = gtk4::ScrolledWindow::new();
        scroller.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::Never);
        scroller.set_child(Some(&crumbs));
        scroller.set_hexpand(true);

        let input = gtk4::Entry::new();
        input.add_css_class("wallpaper-address-input");
        input.set_has_frame(false);
        input.set_hexpand(true);
        input.set_attributes(&{
            let attributes = pango::AttrList::new();
            attributes.insert(pango::AttrFontDesc::new(&text::font(
                Family::Main,
                pixel_size::SMALL as f64,
                "wght=450",
            )));
            attributes
        });

        let pages = gtk4::Stack::new();
        pages.set_hexpand(true);
        pages.add_named(&scroller, Some("breadcrumb"));
        pages.add_named(&input, Some("input"));

        let edit_symbol = text::symbol("edit", pixel_size::LARGER as f64);
        text::set_color(&edit_symbol, "colOnLayer2");
        let edit = RippleButton::new(theme);
        edit.set_content(
            &Centred::integral(&edit_symbol),
            BUTTON_PADDING.0,
            BUTTON_PADDING.1,
        );
        edit.set_valign(gtk4::Align::Center);
        let tip = Tooltip::new(&edit, theme, tooltip::Kind::Styled);
        tip.set_text(&tr("Edit directory"));
        tooltip::hover_delay(&edit, &tip, 0);

        let widget = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        widget.add_css_class("wallpaper-address");
        widget.set_margin_start(INSET);
        widget.set_margin_end(INSET);
        widget.set_margin_top(INSET);
        widget.set_margin_bottom(INSET);
        widget.append(&up);
        widget.append(&pages);
        widget.append(&edit);

        let bar = Rc::new(AddressBar {
            widget,
            theme: theme.clone(),
            crumbs,
            scroller,
            input,
            pages,
            edit,
            edit_symbol,
            directory: RefCell::new(String::new()),
            shown_path: RefCell::new(String::new()),
            breadcrumb: Cell::new(true),
            navigate: navigate.clone(),
            _tip: tip,
        });

        up.connect_down({
            let bar = Rc::downgrade(&bar);
            move || {
                if let Some(bar) = bar.upgrade() {
                    let directory = bar.directory.borrow().clone();
                    let parent = parent_directory(&directory);
                    (bar.navigate)(&parent);
                }
            }
        });
        bar.edit.connect_down({
            let bar = Rc::downgrade(&bar);
            move || {
                if let Some(bar) = bar.upgrade() {
                    bar.set_breadcrumb(!bar.breadcrumb.get());
                }
            }
        });
        bar.input.connect_activate({
            let bar = Rc::downgrade(&bar);
            move |input| {
                if let Some(bar) = bar.upgrade() {
                    let text = input.text().to_string();
                    (bar.navigate)(if text.is_empty() { "/" } else { &text });
                    bar.set_breadcrumb(true);
                }
            }
        });
        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed({
            let bar = Rc::downgrade(&bar);
            move |_, key, _, _| {
                let Some(bar) = bar.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if key == gdk::Key::Escape && !bar.breadcrumb.get() {
                    bar.set_breadcrumb(true);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        });
        bar.input.add_controller(keys);
        bar
    }

    pub fn editing(&self) -> bool {
        !self.breadcrumb.get()
    }

    pub fn focus_input(&self) {
        self.set_breadcrumb(false);
        self.input.grab_focus();
    }

    pub fn set_directory(self: &Rc<Self>, directory: &str) {
        self.directory.replace(directory.to_owned());
        let keep = self.shown_path.borrow().starts_with(directory) && !directory.is_empty();
        if !keep || self.shown_path.borrow().is_empty() {
            self.shown_path.replace(directory.to_owned());
        }
        if self.breadcrumb.get() {
            self.input.set_text(directory);
        }
        self.rebuild();
    }

    fn set_breadcrumb(&self, shown: bool) {
        self.breadcrumb.set(shown);
        self.input.set_text(&self.directory.borrow());
        self.pages
            .set_visible_child_name(if shown { "breadcrumb" } else { "input" });
        self.edit.set_toggled(!shown);
        text::set_color(
            &self.edit_symbol,
            if shown { "colOnLayer2" } else { "colOnPrimary" },
        );
        if !shown {
            self.input.grab_focus();
            self.input.set_position(-1);
        }
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.crumbs.first_child() {
            self.crumbs.remove(&child);
        }
        let shown = self.shown_path.borrow().clone();
        let parts: Vec<&str> = shown.split('/').collect();
        let directory = self.directory.borrow().clone();
        let current = if directory.trim() == "/" {
            0
        } else {
            directory.split('/').count() - 1
        };
        let last = parts.len() - 1;
        let mut buttons = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let label = text::styled(if index == 0 { "/" } else { part });
            let toggled = index == current;
            text::set_color(
                &label,
                if toggled {
                    "colOnPrimary"
                } else {
                    "colOnSecondaryContainer"
                },
            );
            let width = label.measure(gtk4::Orientation::Horizontal, -1).1 as f64;
            let height = CRUMB_TEXT_HEIGHT + CRUMB_PADDING.1 * 2.0;
            let button = GroupButton::new(&self.theme, width + CRUMB_PADDING.0 * 2.0, height);
            button.set_look(GroupLook {
                background: |theme| theme.colors.col_secondary_container,
                hover: |theme| theme.colors.col_secondary_container_hover,
                active: |theme| theme.colors.col_secondary_container_active,
                ..GroupLook::default()
            });
            button.set_bounce(false);
            button.set_toggled(toggled);
            let half = height / 2.0;
            let side = |edge: bool| {
                if toggled || edge {
                    half
                } else {
                    rounding::UNSHARPENMORE as f64
                }
            };
            button.set_side_radii(side(index == 0), side(index == last));
            button.set_content(&Centred::integral(&label));
            let target = parts[..=index].join("/");
            button.connect_clicked({
                let navigate = self.navigate.clone();
                move || navigate(if target.is_empty() { "/" } else { &target })
            });
            self.crumbs.append(&button);
            buttons.push(button);
        }
        if let Some(button) = buttons.get(current).cloned() {
            let scroller = self.scroller.clone();
            glib::idle_add_local_once(move || {
                let Some(bounds) = button.compute_bounds(&scroller) else {
                    return;
                };
                let adjustment = scroller.hadjustment();
                let right = bounds.x() as f64 + bounds.width() as f64 + adjustment.value();
                if right > adjustment.value() + adjustment.page_size() {
                    adjustment.set_value(right - adjustment.page_size());
                }
            });
        }
    }
}

pub fn parent_directory(directory: &str) -> String {
    let trimmed = directory.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => parent.to_owned(),
        _ => "/".to_owned(),
    }
}
