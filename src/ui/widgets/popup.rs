use gtk4::prelude::*;

use crate::core::config::Config;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text;

const ELEVATION_MARGIN: i32 = 10;

#[derive(Clone, Copy)]
pub enum Bar {
    Horizontal { height: i32 },
    Vertical { width: i32, right: bool },
}

impl Bar {
    pub fn of(config: &Config) -> Self {
        if config.vertical {
            Bar::Vertical {
                width: config.vertical_bar_width(),
                right: config.bottom,
            }
        } else {
            Bar::Horizontal {
                height: config.bar_height(),
            }
        }
    }
}

#[derive(Clone)]
pub struct Popup {
    popover: gtk4::Popover,
    bar: Bar,
}

impl Popup {
    pub fn new(
        target: &impl IsA<gtk4::Widget>,
        bar: Bar,
        content: &impl IsA<gtk4::Widget>,
    ) -> Self {
        let popover = gtk4::Popover::new();
        popover.set_parent(target.as_ref());
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_has_arrow(false);
        popover.set_autohide(false);
        popover.set_can_focus(false);
        popover.add_css_class("bar-popup");
        popover.set_child(Some(content.as_ref()));
        target.as_ref().connect_destroy({
            let popover = popover.downgrade();
            move |_| {
                if let Some(popover) = popover.upgrade() {
                    popover.unparent();
                }
            }
        });
        Popup { popover, bar }
    }

    pub fn show(&self, visible: bool) {
        if !visible {
            self.popover.popdown();
            return;
        }
        let Some(target) = self.popover.parent() else {
            return;
        };
        place(&self.popover, &target, self.bar);
        self.popover.popup();
    }

    pub fn popover(&self) -> &gtk4::Popover {
        &self.popover
    }
}

pub fn attach(target: &impl IsA<gtk4::Widget>, bar: Bar, content: &impl IsA<gtk4::Widget>) {
    let popup = Popup::new(target, bar, content);
    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let popup = popup.clone();
        move |_, _, _| popup.show(true)
    });
    hover.connect_leave(move |_| popup.show(false));
    target.as_ref().add_controller(hover);
}

fn place(popover: &gtk4::Popover, target: &gtk4::Widget, bar: Bar) {
    let root = target.root();
    let bounds = root.as_ref().and_then(|root| target.compute_bounds(root));
    match bar {
        Bar::Horizontal { height } => {
            let top = bounds.map_or(0.0, |bounds| bounds.y());
            popover.set_position(gtk4::PositionType::Bottom);
            popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
                ELEVATION_MARGIN,
                -top.round() as i32,
                target.width(),
                height + ELEVATION_MARGIN,
            )));
        }
        Bar::Vertical { width, right } => {
            let left = bounds.map_or(0.0, |bounds| bounds.x()).round() as i32;
            let (x, position) = if right {
                let window = root.map_or(0, |root| root.width());
                (
                    window - width - ELEVATION_MARGIN - left,
                    gtk4::PositionType::Left,
                )
            } else {
                (-left, gtk4::PositionType::Right)
            };
            popover.set_position(position);
            popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
                x,
                ELEVATION_MARGIN,
                width + ELEVATION_MARGIN,
                target.height(),
            )));
        }
    }
}

pub fn column() -> gtk4::Box {
    gtk4::Box::new(gtk4::Orientation::Vertical, 4)
}

pub fn header(icon: &str, label: &str) -> gtk4::Widget {
    header_with_label(icon, label).0
}

pub fn header_with_label(icon: &str, label: &str) -> (gtk4::Widget, gtk4::Label) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    let symbol = text::symbol(icon, pixel_size::LARGE as f64);
    text::set_font(
        &symbol,
        text::Family::Material,
        pixel_size::LARGE as f64,
        &format!("FILL=0,opsz={},wght=600", pixel_size::LARGE),
    );
    text::set_color(&symbol, "colOnSurfaceVariant");
    row.append(&Centred::new(&symbol));

    let name = text::styled_sized(label, pixel_size::NORMAL);
    text::set_color(&name, "colOnSurfaceVariant");
    name.set_xalign(0.0);
    row.append(&Centred::new(&name));

    (row.upcast(), name)
}

pub fn value(icon: &str, label: &str, value: &str) -> (gtk4::Widget, gtk4::Label) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    let symbol = text::symbol(icon, pixel_size::LARGE as f64);
    text::set_color(&symbol, "colOnSurfaceVariant");
    row.append(&symbol);

    let name = text::styled(label);
    text::set_color(&name, "colOnSurfaceVariant");
    name.set_xalign(0.0);
    row.append(&name);

    let reading = text::styled(value);
    text::set_color(&reading, "colOnSurfaceVariant");
    reading.set_xalign(1.0);
    reading.set_hexpand(true);
    reading.set_visible(!value.is_empty());
    row.append(&reading);

    (row.upcast(), reading)
}

pub fn note(value: &str) -> gtk4::Label {
    let label = text::styled(value);
    text::set_color(&label, "colOnSurfaceVariant");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}
