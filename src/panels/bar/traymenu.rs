use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::platform::dbus::dbusmenu::{self, Entry, Toggle};
use crate::ui::anim::{self, EXPRESSIVE_EFFECTS};
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::layout::viewport::Viewport;

const ENTRY_HEIGHT: i32 = 36;
const ICON: i32 = 20;
const PIN_ICON: f64 = 18.0;
const HORIZONTAL_PADDING: i32 = 12;
const SPACING: i32 = 8;
const PADDING: i32 = 4;
const ELEVATION_MARGIN: i32 = 10;
const BORDER: i32 = 1;
const RESIZE_MILLIS: f64 = 300.0;
const FADE_MILLIS: f64 = 200.0;
const MENU_CLASS: &str = "tray-menu";

pub struct Pin {
    pub is_pinned: bool,
    pub on_toggle: Rc<dyn Fn()>,
}

struct Menu {
    popover: gtk4::Popover,
    frame: Viewport,
    pages: RefCell<Vec<gtk4::Widget>>,
    generation: Cell<u64>,
    width: Rc<anim::Motion>,
    height: Rc<anim::Motion>,
    session: gio::DBusConnection,
    service: String,
    path: String,
    theme: SharedTheme,
}

pub fn open(
    anchor: &gtk4::Widget,
    session: &gio::DBusConnection,
    service: &str,
    path: &str,
    theme: &SharedTheme,
    pin: Pin,
    side: gtk4::PositionType,
) {
    let mut child = anchor.first_child();
    while let Some(widget) = child {
        if widget.has_css_class(MENU_CLASS) {
            return;
        }
        child = widget.next_sibling();
    }
    let popover = gtk4::Popover::new();
    popover.set_parent(anchor);
    let handler: Rc<Cell<Option<glib::SignalHandlerId>>> = Rc::default();
    let id = anchor.connect_destroy({
        let popover = popover.downgrade();
        let handler = handler.clone();
        move |_| {
            handler.take();
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
                popover.unparent();
            }
        }
    });
    handler.set(Some(id));
    let detach = {
        let anchor = anchor.downgrade();
        move || {
            if let (Some(anchor), Some(id)) = (anchor.upgrade(), handler.take()) {
                anchor.disconnect(id);
            }
        }
    };
    popover.set_position(side);
    popover.set_has_arrow(false);
    if side == gtk4::PositionType::Bottom {
        let top = anchor
            .root()
            .and_then(|root| anchor.compute_bounds(&root))
            .map_or(0.0, |bounds| bounds.y());
        popover.set_pointing_to(Some(&gdk::Rectangle::new(
            0,
            -top.round() as i32,
            anchor.width(),
            crate::core::config::BASE_BAR_HEIGHT,
        )));
    } else {
        popover.set_pointing_to(Some(&gdk::Rectangle::new(
            0,
            0,
            anchor.width(),
            anchor.height(),
        )));
    }
    popover.add_css_class(MENU_CLASS);

    let frame = Viewport::new();
    frame.set_margin_start(PADDING - BORDER);
    frame.set_margin_end(PADDING - BORDER);
    frame.set_margin_top(PADDING - BORDER);
    frame.set_margin_bottom(PADDING - BORDER);
    let background = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    background.add_css_class("tray-menu-background");
    background.set_halign(gtk4::Align::Start);
    background.set_valign(gtk4::Align::Start);
    background.append(&frame);
    let window = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    window.set_margin_start(ELEVATION_MARGIN);
    window.set_margin_end(ELEVATION_MARGIN);
    window.set_margin_top(ELEVATION_MARGIN);
    window.set_margin_bottom(ELEVATION_MARGIN);
    window.append(&background);
    popover.set_child(Some(&window));

    let back_gesture = gtk4::GestureClick::new();
    back_gesture.set_button(0);
    frame.add_controller(back_gesture.clone());

    let menu = Rc::new(Menu {
        width: anim::Motion::new(&frame, 0.0, RESIZE_MILLIS, anim::EMPHASIZED),
        height: anim::Motion::new(&frame, 0.0, RESIZE_MILLIS, anim::EMPHASIZED),
        popover,
        frame,
        pages: RefCell::new(Vec::new()),
        generation: Cell::new(0),
        session: session.clone(),
        service: service.to_owned(),
        path: path.to_owned(),
        theme: theme.clone(),
    });

    back_gesture.connect_pressed({
        let menu = Rc::downgrade(&menu);
        move |gesture, _, _, _| {
            let Some(menu) = menu.upgrade() else {
                return;
            };
            if matches!(gesture.current_button(), 3 | 8) && menu.pages.borrow().len() > 1 {
                menu.pop();
            }
        }
    });

    glib::spawn_future_local(async move {
        dbusmenu::about_to_show(&menu.session, &menu.service, &menu.path, 0).await;
        let Some(root) = dbusmenu::layout(&menu.session, &menu.service, &menu.path, 0)
            .await
            .filter(|_| {
                menu.popover
                    .parent()
                    .is_some_and(|anchor| anchor.is_mapped())
            })
        else {
            detach();
            menu.popover.unparent();
            return;
        };
        let page = page(&menu, &root, false, Some(pin));
        menu.push(page, true);
        fade(&menu.frame);
        menu.popover.connect_closed({
            let owner = RefCell::new(Some(menu.clone()));
            move |popover| {
                detach();
                popover.unparent();
                owner.take();
            }
        });
        menu.popover.popup();
    });
}

impl Menu {
    fn push(self: &Rc<Self>, page: gtk4::Widget, is_first: bool) {
        self.frame.show_child(&page);
        self.pages.borrow_mut().push(page.clone());
        self.resize(&page, is_first);
        fade(&page);
    }

    fn pop(self: &Rc<Self>) {
        let page = {
            let mut pages = self.pages.borrow_mut();
            if let Some(top) = pages.pop() {
                top.unparent();
                self.generation.set(self.generation.get() + 1);
            }
            pages.last().cloned()
        };
        let Some(page) = page else {
            return;
        };
        self.frame.show_child(&page);
        self.resize(&page, false);
        fade(&page);
    }

    fn resize(self: &Rc<Self>, page: &gtk4::Widget, is_first: bool) {
        let depth = self.pages.borrow().len();
        if let Some(pin) = self
            .pages
            .borrow()
            .first()
            .and_then(|root| root.first_child())
            .filter(|child| child.has_css_class("pin-entry"))
        {
            pin.set_visible(depth == 1);
        }
        let size = |page: &gtk4::Widget| {
            let is_hidden = !page.get_visible();
            page.set_visible(true);
            let width = page.measure(gtk4::Orientation::Horizontal, -1).1;
            let height = page.measure(gtk4::Orientation::Vertical, width).1;
            page.set_visible(!is_hidden);
            (width, height)
        };
        let (widest, tallest) = self
            .pages
            .borrow()
            .iter()
            .map(size)
            .fold((0, 0), |(w, h), (pw, ph)| (w.max(pw), h.max(ph)));
        if let Some(window) = self.popover.child() {
            window.set_size_request(widest + PADDING * 2, tallest + PADDING * 2);
        }
        let (width, height) = (widest as f64, size(page).1 as f64);
        if is_first {
            self.width.jump(width);
            self.height.jump(height);
            self.frame.set_size(width as i32, height as i32);
            return;
        }
        self.width.to(width);
        self.height.to(height);
        let menu = Rc::downgrade(self);
        self.frame.add_tick_callback(move |frame, _| {
            let Some(menu) = menu.upgrade() else {
                return glib::ControlFlow::Break;
            };
            frame.set_size(menu.width.get() as i32, menu.height.get() as i32);
            menu.popover.present();
            if menu.width.is_running() || menu.height.is_running() {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    fn close(&self) {
        self.popover.popdown();
    }

    fn is_current(&self, generation: u64) -> bool {
        self.popover.is_mapped() && self.generation.get() == generation
    }
}

fn fade(widget: &impl IsA<gtk4::Widget>) {
    let widget = widget.as_ref().clone();
    widget.set_opacity(0.0);
    let opacity = anim::Motion::new(&widget, 0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS);
    opacity.to(1.0);
    widget.clone().add_tick_callback(move |widget, _| {
        widget.set_opacity(opacity.get());
        if opacity.is_running() {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

fn page(menu: &Rc<Menu>, parent: &Entry, is_submenu: bool, pin: Option<Pin>) -> gtk4::Widget {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    if is_submenu {
        let button = entry_button(menu);
        let content = Row::new(SPACING);
        content.append(&text::symbol("chevron_left", ICON as f64));
        let label = text::styled(&tr("Back"));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        content.append(&label);
        button.set_content(&Centered::filling_width(&content), HORIZONTAL_PADDING, 0);
        button.connect_clicked({
            let menu = Rc::downgrade(menu);
            move |_| {
                if let Some(menu) = menu.upgrade() {
                    menu.pop();
                }
            }
        });
        column.append(&button);
    }

    if let Some(pin) = pin.filter(|_| !is_submenu) {
        let button = entry_button(menu);
        let content = Row::new(SPACING);
        content.append(&text::symbol("push_pin", PIN_ICON));
        let label = text::styled(&tr(if pin.is_pinned { "Unpin" } else { "Pin" }));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        content.append(&label);
        button.set_content(&Centered::filling_width(&content), HORIZONTAL_PADDING, 0);
        button.connect_clicked({
            let on_toggle = pin.on_toggle.clone();
            move |_| on_toggle()
        });
        button.add_css_class("pin-entry");
        column.append(&button);
    }

    let rule = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    rule.add_css_class("menu-rule");
    rule.set_size_request(-1, 1);
    rule.set_margin_top(4);
    rule.set_margin_bottom(4);
    column.append(&rule);

    let has_icon_column = parent.children.iter().any(has_icon);
    let has_toggle_column = parent
        .children
        .iter()
        .any(|entry| !matches!(entry.toggle, Toggle::None));
    for entry in &parent.children {
        if entry.is_separator {
            let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            line.add_css_class("menu-separator");
            line.set_size_request(-1, 1);
            line.set_margin_top(4);
            line.set_margin_bottom(4);
            column.append(&line);
            continue;
        }
        column.append(&row(menu, entry, has_toggle_column, has_icon_column));
    }
    column.upcast()
}

fn entry_button(menu: &Rc<Menu>) -> RippleButton {
    let button = RippleButton::new(&menu.theme);
    button.set_radius((rounding::WINDOW_ROUNDING - PADDING) as f64);
    button.set_size_request(-1, ENTRY_HEIGHT);
    button.set_hexpand(true);
    button
}

fn has_icon(entry: &Entry) -> bool {
    !entry.icon_name.is_empty() || !entry.icon_data.is_empty()
}

fn row(
    menu: &Rc<Menu>,
    entry: &Entry,
    has_toggle_column: bool,
    has_icon_column: bool,
) -> gtk4::Widget {
    let content = Row::new(SPACING);
    content.set_valign(gtk4::Align::Center);

    if has_toggle_column || !matches!(entry.toggle, Toggle::None) {
        content.append(&toggle_mark(menu, entry.toggle));
    }
    if has_icon_column || has_icon(entry) {
        content.append(&icon(entry));
    }

    let label = text::styled_sized(&entry.label, pixel_size::SMALLIE);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    content.append(&label);

    if entry.has_children {
        content.append(&text::symbol("chevron_right", ICON as f64));
    } else {
        content.append(&gtk4::Box::new(gtk4::Orientation::Horizontal, 0));
    }

    let button = entry_button(menu);
    button.set_content(&Centered::filling_width(&content), HORIZONTAL_PADDING, 0);

    let id = entry.id;
    let has_children = entry.has_children;
    let weak_menu = Rc::downgrade(menu);
    button.connect_clicked(move |_| {
        let Some(menu) = weak_menu.upgrade() else {
            return;
        };
        let generation = menu.generation.get() + 1;
        menu.generation.set(generation);
        let session = menu.session.clone();
        let service = menu.service.clone();
        let path = menu.path.clone();
        let weak_menu = weak_menu.clone();
        glib::spawn_future_local(async move {
            if !has_children {
                dbusmenu::click(&session, &service, &path, id).await;
                if let Some(menu) = weak_menu.upgrade() {
                    menu.close();
                }
                return;
            }
            dbusmenu::about_to_show(&session, &service, &path, id).await;
            let upgrade_current = || {
                weak_menu
                    .upgrade()
                    .filter(|menu| menu.is_current(generation))
            };
            if upgrade_current().is_none() {
                return;
            }
            if let Some(submenu) = dbusmenu::layout(&session, &service, &path, id).await
                && let Some(menu) = upgrade_current()
            {
                let page = page(&menu, &submenu, true, None);
                menu.push(page, false);
            }
        });
    });

    button.upcast()
}

fn toggle_mark(menu: &Rc<Menu>, toggle: Toggle) -> gtk4::Widget {
    let slot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    slot.set_size_request(ICON, ICON);
    slot.set_valign(gtk4::Align::Center);
    match toggle {
        Toggle::Check(true) => {
            slot.append(&text::symbol("check", ICON as f64));
        }
        Toggle::Radio(is_checked) => slot.append(&radio(&menu.theme, is_checked)),
        Toggle::Check(false) | Toggle::None => {}
    }
    slot.upcast()
}

fn radio(theme: &SharedTheme, is_checked: bool) -> gtk4::Widget {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(ICON);
    area.set_content_height(ICON);
    let theme = theme.clone();
    area.set_draw_func(move |_, cr, width, height| {
        let theme = theme.borrow();
        let (x, y) = (width as f64 / 2.0, height as f64 / 2.0);
        let ring = if is_checked {
            theme.colors.col_primary
        } else {
            theme.m3.on_surface_variant
        };
        cr.set_source_rgba(
            ring.red() as f64,
            ring.green() as f64,
            ring.blue() as f64,
            ring.alpha() as f64,
        );
        cr.set_line_width(2.0);
        cr.arc(
            x,
            y,
            ICON as f64 / 2.0 - 1.0,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        let _ = cr.stroke();
        if is_checked {
            let dot = theme.colors.col_primary;
            cr.set_source_rgba(
                dot.red() as f64,
                dot.green() as f64,
                dot.blue() as f64,
                dot.alpha() as f64,
            );
            cr.arc(x, y, 5.0, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.fill();
        }
    });
    area.upcast()
}

fn icon(entry: &Entry) -> gtk4::Widget {
    let image = gtk4::Image::new();
    image.set_pixel_size(ICON);
    image.set_size_request(ICON, ICON);
    image.set_valign(gtk4::Align::Center);
    if !entry.icon_name.is_empty() {
        image.set_icon_name(Some(&entry.icon_name));
    } else if !entry.icon_data.is_empty() {
        let bytes = glib::Bytes::from(&entry.icon_data);
        if let Ok(texture) = gdk::Texture::from_bytes(&bytes) {
            image.set_paintable(Some(&texture));
        }
    }
    image.upcast()
}
