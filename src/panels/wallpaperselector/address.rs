use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::group::{GroupButton, Look as GroupLook};
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

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
    input_field: gtk4::Entry,
    pages: gtk4::Stack,
    edit_button: RippleButton,
    edit_symbol: gtk4::Label,
    directory: RefCell<String>,
    breadcrumb_path: RefCell<String>,
    shows_breadcrumb: Cell<bool>,
    on_navigate: Navigate,
    _tip: Rc<Tooltip>,
}

impl AddressBar {
    pub fn new(theme: &SharedTheme, on_navigate: impl Fn(&str) + 'static) -> Rc<Self> {
        let on_navigate: Navigate = Rc::new(on_navigate);

        let up_symbol = text::symbol("drive_folder_upload", pixel_size::LARGER as f64);
        text::set_color(&up_symbol, "m3onBackground");
        let up_button = RippleButton::new(theme);
        up_button.set_content(
            &Centered::integral(&up_symbol),
            BUTTON_PADDING.0,
            BUTTON_PADDING.1,
        );
        up_button.set_valign(gtk4::Align::Center);

        let crumbs = gtk4::Box::new(gtk4::Orientation::Horizontal, CRUMB_SPACING);
        crumbs.set_valign(gtk4::Align::Center);
        let scroller = gtk4::ScrolledWindow::new();
        scroller.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::Never);
        scroller.set_child(Some(&crumbs));
        scroller.set_hexpand(true);

        let input_field = gtk4::Entry::new();
        input_field.add_css_class("wallpaper-address-input");
        input_field.set_has_frame(false);
        input_field.set_hexpand(true);
        input_field.set_attributes(&{
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
        pages.add_named(&input_field, Some("input"));

        let edit_symbol = text::symbol("edit", pixel_size::LARGER as f64);
        text::set_color(&edit_symbol, "colOnLayer2");
        let edit_button = RippleButton::new(theme);
        edit_button.set_content(
            &Centered::integral(&edit_symbol),
            BUTTON_PADDING.0,
            BUTTON_PADDING.1,
        );
        edit_button.set_valign(gtk4::Align::Center);
        let tip = Tooltip::new(&edit_button, theme, tooltip::Kind::Styled);
        tip.set_text(&tr("Edit directory"));
        tooltip::hover_delay(&edit_button, &tip, 0);

        let widget = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        widget.add_css_class("wallpaper-address");
        widget.set_margin_start(INSET);
        widget.set_margin_end(INSET);
        widget.set_margin_top(INSET);
        widget.set_margin_bottom(INSET);
        widget.append(&up_button);
        widget.append(&pages);
        widget.append(&edit_button);

        let bar = Rc::new(AddressBar {
            widget,
            theme: theme.clone(),
            crumbs,
            scroller,
            input_field,
            pages,
            edit_button,
            edit_symbol,
            directory: RefCell::new(String::new()),
            breadcrumb_path: RefCell::new(String::new()),
            shows_breadcrumb: Cell::new(true),
            on_navigate: on_navigate.clone(),
            _tip: tip,
        });

        up_button.connect_down({
            let bar = Rc::downgrade(&bar);
            move || {
                if let Some(bar) = bar.upgrade() {
                    let directory = bar.directory.borrow().clone();
                    let parent = parent_directory(&directory);
                    (bar.on_navigate)(&parent);
                }
            }
        });
        bar.edit_button.connect_down({
            let bar = Rc::downgrade(&bar);
            move || {
                if let Some(bar) = bar.upgrade() {
                    bar.set_shows_breadcrumb(!bar.shows_breadcrumb.get());
                }
            }
        });
        bar.input_field.connect_activate({
            let bar = Rc::downgrade(&bar);
            move |input_field| {
                if let Some(bar) = bar.upgrade() {
                    let text = input_field.text().to_string();
                    (bar.on_navigate)(if text.is_empty() { "/" } else { &text });
                    bar.set_shows_breadcrumb(true);
                }
            }
        });
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.connect_key_pressed({
            let bar = Rc::downgrade(&bar);
            move |_, key, _, _| {
                let Some(bar) = bar.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if key == gdk::Key::Escape && !bar.shows_breadcrumb.get() {
                    bar.set_shows_breadcrumb(true);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        });
        bar.input_field.add_controller(key_controller);
        bar
    }

    pub fn is_editing(&self) -> bool {
        !self.shows_breadcrumb.get()
    }

    pub fn focus_input(&self) {
        self.set_shows_breadcrumb(false);
        self.input_field.grab_focus();
    }

    pub fn set_directory(self: &Rc<Self>, directory: &str) {
        self.directory.replace(directory.to_owned());
        let should_keep =
            self.breadcrumb_path.borrow().starts_with(directory) && !directory.is_empty();
        if !should_keep || self.breadcrumb_path.borrow().is_empty() {
            self.breadcrumb_path.replace(directory.to_owned());
        }
        if self.shows_breadcrumb.get() {
            self.input_field.set_text(directory);
        }
        self.rebuild();
    }

    fn set_shows_breadcrumb(&self, shows_breadcrumb: bool) {
        self.shows_breadcrumb.set(shows_breadcrumb);
        self.input_field.set_text(&self.directory.borrow());
        self.pages.set_visible_child_name(if shows_breadcrumb {
            "breadcrumb"
        } else {
            "input"
        });
        self.edit_button.set_toggled(!shows_breadcrumb);
        text::set_color(
            &self.edit_symbol,
            if shows_breadcrumb {
                "colOnLayer2"
            } else {
                "colOnPrimary"
            },
        );
        if !shows_breadcrumb {
            self.input_field.grab_focus();
            self.input_field.set_position(-1);
        }
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.crumbs.first_child() {
            self.crumbs.remove(&child);
        }
        let breadcrumb_path = self.breadcrumb_path.borrow().clone();
        let parts: Vec<&str> = breadcrumb_path.split('/').collect();
        let directory = self.directory.borrow().clone();
        let current_index = if directory.trim() == "/" {
            0
        } else {
            directory.split('/').count() - 1
        };
        let last_index = parts.len() - 1;
        let mut buttons = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            let label = text::styled(if index == 0 { "/" } else { part });
            let is_toggled = index == current_index;
            text::set_color(
                &label,
                if is_toggled {
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
            button.set_toggled(is_toggled);
            let half_height = height / 2.0;
            let side_radius = |is_edge: bool| {
                if is_toggled || is_edge {
                    half_height
                } else {
                    rounding::UNSHARPENMORE as f64
                }
            };
            button.set_side_radii(side_radius(index == 0), side_radius(index == last_index));
            button.set_content(&Centered::integral(&label));
            let target = parts[..=index].join("/");
            button.connect_clicked({
                let on_navigate = self.on_navigate.clone();
                move || on_navigate(if target.is_empty() { "/" } else { &target })
            });
            self.crumbs.append(&button);
            buttons.push(button);
        }
        if let Some(button) = buttons.get(current_index).cloned() {
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
    let path = directory.trim_end_matches('/');
    match path.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => parent.to_owned(),
        _ => "/".to_owned(),
    }
}
