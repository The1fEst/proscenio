use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::dbusmenu::{self, Entry, Toggle};
use crate::ui::anim::{self, EXPRESSIVE_EFFECTS};
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;
use crate::ui::widgets::viewport::Viewport;

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

pub struct Pin {
    pub pinned: bool,
    pub toggle: Rc<dyn Fn()>,
}

struct Menu {
    popover: gtk4::Popover,
    frame: Viewport,
    pages: RefCell<Vec<gtk4::Widget>>,
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
    let popover = gtk4::Popover::new();
    popover.set_parent(anchor);
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
    popover.add_css_class("tray-menu");
    popover.connect_closed(|popover| popover.unparent());

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

    let back = gtk4::GestureClick::new();
    back.set_button(0);
    frame.add_controller(back.clone());

    let menu = Rc::new(Menu {
        width: anim::Motion::new(&frame, 0.0, RESIZE_MILLIS, anim::EMPHASIZED),
        height: anim::Motion::new(&frame, 0.0, RESIZE_MILLIS, anim::EMPHASIZED),
        popover,
        frame,
        pages: RefCell::new(Vec::new()),
        session: session.clone(),
        service: service.to_owned(),
        path: path.to_owned(),
        theme: theme.clone(),
    });

    back.connect_pressed({
        let menu = menu.clone();
        move |gesture, _, _, _| {
            if matches!(gesture.current_button(), 3 | 8) && menu.pages.borrow().len() > 1 {
                menu.pop();
            }
        }
    });

    glib::spawn_future_local(async move {
        dbusmenu::about_to_show(&menu.session, &menu.service, &menu.path, 0).await;
        let Some(root) = dbusmenu::layout(&menu.session, &menu.service, &menu.path, 0).await else {
            menu.popover.unparent();
            return;
        };
        let page = page(&menu, &root, false, Some(pin));
        menu.push(page, true);
        fade(&menu.frame);
        menu.popover.popup();
    });
}

impl Menu {
    fn push(self: &Rc<Self>, page: gtk4::Widget, first: bool) {
        self.frame.show_child(&page);
        self.pages.borrow_mut().push(page.clone());
        self.resize(&page, first);
        fade(&page);
    }

    fn pop(self: &Rc<Self>) {
        let page = {
            let mut pages = self.pages.borrow_mut();
            if let Some(top) = pages.pop() {
                top.unparent();
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

    fn resize(self: &Rc<Self>, page: &gtk4::Widget, first: bool) {
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
            let hidden = !page.get_visible();
            page.set_visible(true);
            let width = page.measure(gtk4::Orientation::Horizontal, -1).1;
            let height = page.measure(gtk4::Orientation::Vertical, width).1;
            page.set_visible(!hidden);
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
        if first {
            self.width.jump(width);
            self.height.jump(height);
            self.frame.set_size(width as i32, height as i32);
            return;
        }
        self.width.to(width);
        self.height.to(height);
        let menu = self.clone();
        self.frame.add_tick_callback(move |frame, _| {
            frame.set_size(menu.width.get() as i32, menu.height.get() as i32);
            menu.popover.present();
            if menu.width.running() || menu.height.running() {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    fn close(&self) {
        self.popover.popdown();
    }
}

fn fade(widget: &impl IsA<gtk4::Widget>) {
    let widget = widget.as_ref().clone();
    widget.set_opacity(0.0);
    let opacity = anim::Motion::new(&widget, 0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS);
    opacity.to(1.0);
    widget.clone().add_tick_callback(move |widget, _| {
        widget.set_opacity(opacity.get());
        if opacity.running() {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

fn page(menu: &Rc<Menu>, parent: &Entry, submenu: bool, pin: Option<Pin>) -> gtk4::Widget {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    if submenu {
        let button = entry_button(menu);
        let content = Row::new(SPACING);
        content.append(&text::symbol("chevron_left", ICON as f64));
        let label = text::styled("Back");
        label.set_xalign(0.0);
        label.set_hexpand(true);
        content.append(&label);
        button.set_content(&Centred::filling_width(&content), HORIZONTAL_PADDING, 0);
        button.connect_down({
            let menu = menu.clone();
            move || menu.pop()
        });
        column.append(&button);
    }

    if let Some(pin) = pin.filter(|_| !submenu) {
        let button = entry_button(menu);
        let content = Row::new(SPACING);
        content.append(&text::symbol("push_pin", PIN_ICON));
        let label = text::styled(if pin.pinned { "Unpin" } else { "Pin" });
        label.set_xalign(0.0);
        label.set_hexpand(true);
        content.append(&label);
        button.set_content(&Centred::filling_width(&content), HORIZONTAL_PADDING, 0);
        button.connect_release({
            let toggle = pin.toggle.clone();
            move || toggle()
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

    let icon_column = parent.children.iter().any(has_icon);
    let toggle_column = parent
        .children
        .iter()
        .any(|entry| !matches!(entry.toggle, Toggle::None));
    for entry in &parent.children {
        if entry.separator {
            let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            line.add_css_class("menu-separator");
            line.set_size_request(-1, 1);
            line.set_margin_top(4);
            line.set_margin_bottom(4);
            column.append(&line);
            continue;
        }
        column.append(&row(menu, entry, toggle_column, icon_column));
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

fn row(menu: &Rc<Menu>, entry: &Entry, toggle_column: bool, icon_column: bool) -> gtk4::Widget {
    let content = Row::new(SPACING);
    content.set_valign(gtk4::Align::Center);

    if toggle_column || !matches!(entry.toggle, Toggle::None) {
        content.append(&toggle_mark(menu, entry.toggle));
    }
    if icon_column || has_icon(entry) {
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
    button.set_content(&Centred::filling_width(&content), HORIZONTAL_PADDING, 0);

    let id = entry.id;
    let has_children = entry.has_children;
    let menu = menu.clone();
    button.connect_release(move || {
        let menu = menu.clone();
        glib::spawn_future_local(async move {
            if !has_children {
                dbusmenu::clicked(&menu.session, &menu.service, &menu.path, id).await;
                menu.close();
                return;
            }
            dbusmenu::about_to_show(&menu.session, &menu.service, &menu.path, id).await;
            if let Some(submenu) =
                dbusmenu::layout(&menu.session, &menu.service, &menu.path, id).await
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
        Toggle::Radio(checked) => slot.append(&radio(&menu.theme, checked)),
        Toggle::Check(false) | Toggle::None => {}
    }
    slot.upcast()
}

fn radio(theme: &SharedTheme, checked: bool) -> gtk4::Widget {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(ICON);
    area.set_content_height(ICON);
    let theme = theme.clone();
    area.set_draw_func(move |_, cr, width, height| {
        let theme = theme.borrow();
        let (x, y) = (width as f64 / 2.0, height as f64 / 2.0);
        let ring = if checked {
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
        if checked {
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
