use gtk4::gdk;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::menuitems::MenuItem;
use crate::core::i18n::tr;
use crate::platform::system::appicon;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

const ENTRY_HEIGHT: i32 = 36;
const ICON: i32 = 20;
const HORIZONTAL_PADDING: i32 = 12;
const SPACING: i32 = 8;
const PADDING: i32 = 4;
const ELEVATION: i32 = 10;
const MIN_WIDTH: i32 = 200;
const MAX_LABEL_CHARS: i32 = 28;
const SEPARATOR_MARGIN: i32 = 4;

type OnChoose = Rc<dyn Fn(&MenuItem)>;

enum Icon {
    Symbol(&'static str),
    Theme(String),
}

pub(super) struct ContextMenu {
    theme: SharedTheme,
    parent: gtk4::Widget,
    popover: gtk4::Popover,
    is_open: Cell<bool>,
    on_changed: RefCell<Option<Box<dyn Fn()>>>,
}

impl ContextMenu {
    pub(super) fn new(theme: &SharedTheme, parent: &impl IsA<gtk4::Widget>) -> Rc<Self> {
        let popover = gtk4::Popover::new();
        popover.add_css_class("dock-menu");
        popover.set_has_arrow(false);
        popover.set_position(gtk4::PositionType::Top);
        popover.set_parent(parent);
        crate::ui::unload::unrealize_when_hidden(&popover);
        let menu = Rc::new(ContextMenu {
            theme: theme.clone(),
            parent: parent.as_ref().clone(),
            popover: popover.clone(),
            is_open: Cell::new(false),
            on_changed: RefCell::new(None),
        });
        popover.connect_closed({
            let menu = Rc::downgrade(&menu);
            move |_| {
                if let Some(menu) = menu.upgrade() {
                    menu.is_open.set(false);
                    menu.notify_changed();
                }
            }
        });
        menu
    }

    pub(super) fn connect_changed(&self, action: impl Fn() + 'static) {
        self.on_changed.replace(Some(Box::new(action)));
    }

    pub(super) fn is_open(&self) -> bool {
        self.is_open.get()
    }

    pub(super) fn show(
        &self,
        anchor: &RippleButton,
        items: Vec<MenuItem>,
        on_choose: impl Fn(&MenuItem) + 'static,
    ) {
        let Some(bounds) = anchor.compute_bounds(&self.parent) else {
            return;
        };
        let on_choose: OnChoose = Rc::new(on_choose);
        let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        list.set_margin_start(PADDING);
        list.set_margin_end(PADDING);
        list.set_margin_top(PADDING);
        list.set_margin_bottom(PADDING);
        list.set_size_request(MIN_WIDTH, -1);
        for item in items {
            list.append(&self.build_row(Rc::new(item), &on_choose));
        }
        let background = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        background.add_css_class("tray-menu-background");
        background.append(&list);
        let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        frame.set_margin_start(ELEVATION);
        frame.set_margin_end(ELEVATION);
        frame.set_margin_top(ELEVATION);
        frame.set_margin_bottom(ELEVATION);
        frame.append(&background);
        self.popover.set_child(Some(&frame));
        self.popover.set_pointing_to(Some(&gdk::Rectangle::new(
            bounds.x() as i32,
            0,
            bounds.width() as i32,
            1,
        )));
        self.is_open.set(true);
        self.notify_changed();
        self.popover.popup();
    }

    fn notify_changed(&self) {
        if let Some(on_changed) = self.on_changed.borrow().as_ref() {
            on_changed();
        }
    }

    fn build_row(&self, item: Rc<MenuItem>, on_choose: &OnChoose) -> gtk4::Widget {
        let (icon, label) = match &*item {
            MenuItem::Separator => return build_separator(),
            MenuItem::Window { address, title } => {
                return self.build_window_row(item.clone(), address, title, on_choose);
            }
            MenuItem::NewWindow => (Icon::Symbol("add"), tr("New window")),
            MenuItem::Action { icon, name, .. } => (Icon::Theme(icon.clone()), name.clone()),
            MenuItem::TogglePin { is_pinned: true } => (Icon::Symbol("keep_off"), tr("Unpin")),
            MenuItem::TogglePin { is_pinned: false } => (Icon::Symbol("push_pin"), tr("Pin")),
            MenuItem::CloseWindows { .. } => (Icon::Symbol("close"), tr("Close all windows")),
        };
        let content = Row::new(SPACING);
        content.set_valign(gtk4::Align::Center);
        content.append(&self.build_icon(&icon));
        content.append(&build_label(&label));
        let button = self.build_entry_button();
        button.set_content(&Centered::filling_width(&content), HORIZONTAL_PADDING, 0);
        self.connect_choice(&button, item, on_choose);
        button.upcast()
    }

    fn build_window_row(
        &self,
        item: Rc<MenuItem>,
        address: &str,
        title: &str,
        on_choose: &OnChoose,
    ) -> gtk4::Widget {
        let focus_button = self.build_entry_button();
        focus_button.set_content(
            &Centered::filling_width(&build_label(title)),
            HORIZONTAL_PADDING,
            0,
        );
        self.connect_choice(&focus_button, item, on_choose);

        let close_button = RippleButton::new(&self.theme);
        close_button.set_radius(rounding::FULL as f64);
        close_button.set_size_request(ENTRY_HEIGHT, ENTRY_HEIGHT);
        close_button.set_content(
            &Centered::integral(&text::symbol("close", ICON as f64)),
            0,
            0,
        );
        let close_item = MenuItem::CloseWindows {
            addresses: vec![address.to_owned()],
        };
        self.connect_choice(&close_button, Rc::new(close_item), on_choose);

        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        line.append(&focus_button);
        line.append(&close_button);
        line.upcast()
    }

    fn connect_choice(&self, button: &RippleButton, item: Rc<MenuItem>, on_choose: &OnChoose) {
        button.connect_clicked({
            let popover = self.popover.downgrade();
            let on_choose = on_choose.clone();
            move |_| {
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
                on_choose(&item);
            }
        });
    }

    fn build_entry_button(&self) -> RippleButton {
        let button = RippleButton::new(&self.theme);
        button.set_radius((rounding::WINDOW_ROUNDING - PADDING) as f64);
        button.set_size_request(-1, ENTRY_HEIGHT);
        button.set_hexpand(true);
        button
    }

    fn build_icon(&self, icon: &Icon) -> gtk4::Widget {
        match icon {
            Icon::Symbol(name) => {
                let symbol = text::symbol(name, ICON as f64);
                symbol.set_size_request(ICON, ICON);
                symbol.upcast()
            }
            Icon::Theme(name) => {
                let image = gtk4::Image::new();
                image.set_pixel_size(ICON);
                image.set_size_request(ICON, ICON);
                image.set_valign(gtk4::Align::Center);
                if let Some(display) = gdk::Display::default() {
                    let paintable = appicon::load_icon(
                        &gtk4::IconTheme::for_display(&display),
                        name,
                        "image-missing",
                        ICON,
                        self.parent.scale_factor().max(1),
                    );
                    image.set_paintable(Some(&paintable));
                }
                image.upcast()
            }
        }
    }
}

fn build_label(caption: &str) -> gtk4::Label {
    let label = text::styled_sized(caption, pixel_size::SMALLIE);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(pango::EllipsizeMode::End);
    label.set_max_width_chars(MAX_LABEL_CHARS);
    label
}

fn build_separator() -> gtk4::Widget {
    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    line.add_css_class("menu-separator");
    line.set_size_request(-1, 1);
    line.set_margin_top(SEPARATOR_MARGIN);
    line.set_margin_bottom(SEPARATOR_MARGIN);
    line.upcast()
}
