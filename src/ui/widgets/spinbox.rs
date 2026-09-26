use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::ui::anim;
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::group::pointer_cursor;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::text;

const HEIGHT: i32 = 35;
const VALUE_WIDTH: i32 = 40;
const VALUE_DROP: i32 = 2;
const ICON: f64 = 20.0;
const REPEAT_DELAY: Duration = Duration::from_millis(300);
const REPEAT_INTERVAL: Duration = Duration::from_millis(100);

struct Indicator {
    area: gtk4::Box,
    step: i64,
    fade: anim::Fade,
    hovered: Cell<bool>,
    pressed: Cell<bool>,
    repeated: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
}

pub struct SpinBox {
    pub root: gtk4::Overlay,
    background: Paint,
    field: gtk4::Text,
    theme: SharedTheme,
    value: Cell<i64>,
    from: i64,
    to: i64,
    step: i64,
    decimals: u32,
    changed: RefCell<Option<Rc<dyn Fn(i64)>>>,
    down: Rc<Indicator>,
    up: Rc<Indicator>,
}

impl SpinBox {
    pub fn new(theme: &SharedTheme, from: i64, to: i64, step: i64, decimals: u32) -> Rc<Self> {
        let background = Paint::new(|_, _, _| {});
        let down = Indicator::new("remove", -1);
        let up = Indicator::new("add", 1);

        let field = gtk4::Text::new();
        field.add_css_class("spin-value");
        field.set_alignment(0.5);
        field.set_attributes(Some(&{
            let attributes = gtk4::pango::AttrList::new();
            attributes.insert(gtk4::pango::AttrFontDesc::new(&text::font(
                text::Family::Main,
                pixel_size::SMALL as f64,
                "wght=450",
            )));
            attributes
        }));
        field.set_hexpand(true);
        field.set_valign(gtk4::Align::Center);
        field.set_width_chars(1);
        field.set_margin_top(VALUE_DROP);
        field.set_propagate_text_width(true);
        let value_box = Centred::integral(&field);
        value_box.set_size_request(VALUE_WIDTH, HEIGHT);
        value_box.set_hexpand(true);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.append(&down.area);
        row.append(&value_box);
        row.append(&up.area);

        let root = gtk4::Overlay::new();
        root.set_child(Some(&background));
        root.add_overlay(&row);
        root.set_measure_overlay(&row, true);
        root.set_valign(gtk4::Align::Center);

        let spin = Rc::new(SpinBox {
            root,
            background: background.clone(),
            field: field.clone(),
            theme: theme.clone(),
            value: Cell::new(from),
            from: from.min(to),
            to: from.max(to),
            step,
            decimals,
            changed: RefCell::new(None),
            down,
            up,
        });
        background.set_draw({
            let spin = Rc::downgrade(&spin);
            move |snapshot, width, height| {
                if let Some(spin) = spin.upgrade() {
                    spin.draw(snapshot, width, height);
                }
            }
        });
        for indicator in [&spin.down, &spin.up] {
            spin.wire(indicator);
        }
        field.connect_activate({
            let spin = Rc::downgrade(&spin);
            move |_| {
                if let Some(spin) = spin.upgrade() {
                    spin.finish_typing();
                }
            }
        });
        let focus = gtk4::EventControllerFocus::new();
        focus.connect_leave({
            let spin = Rc::downgrade(&spin);
            move |_| {
                if let Some(spin) = spin.upgrade() {
                    spin.finish_typing();
                }
            }
        });
        field.add_controller(focus);
        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed({
            let spin = Rc::downgrade(&spin);
            move |_, key, _, _| {
                let Some(spin) = spin.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let direction = match key {
                    gtk4::gdk::Key::Up => 1,
                    gtk4::gdk::Key::Down => -1,
                    _ => return glib::Propagation::Proceed,
                };
                spin.commit(spin.value.get() + direction * spin.step, true);
                glib::Propagation::Stop
            }
        });
        field.add_controller(keys);
        spin.show_value();
        spin
    }

    pub fn connect_changed(&self, action: impl Fn(i64) + 'static) {
        self.changed.replace(Some(Rc::new(action)));
    }

    pub fn set_value(&self, value: i64) {
        let value = value.clamp(self.from, self.to);
        if self.value.replace(value) != value {
            self.show_value();
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.root.set_sensitive(enabled);
        self.root.set_opacity(if enabled { 1.0 } else { 0.4 });
    }

    fn factor(&self) -> i64 {
        10_i64.pow(self.decimals)
    }

    fn commit(&self, value: i64, show: bool) {
        let value = value.clamp(self.from, self.to);
        if self.value.replace(value) == value {
            return;
        }
        if show {
            self.show_value();
        }
        let changed = self.changed.borrow().clone();
        if let Some(changed) = changed {
            changed(value);
        }
    }

    fn finish_typing(&self) {
        if let Ok(typed) = self.field.text().trim().parse::<f64>()
            && typed.is_finite()
        {
            self.commit((typed * self.factor() as f64).round() as i64, false);
        }
        self.show_value();
    }

    fn show_value(&self) {
        let shown = self.value.get() as f64 / self.factor() as f64;
        self.field
            .set_text(&format!("{shown:.*}", self.decimals as usize));
    }

    fn wire(self: &Rc<Self>, indicator: &Rc<Indicator>) {
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_enter({
            let spin = Rc::downgrade(self);
            let indicator = Rc::downgrade(indicator);
            move |_, _, _| {
                let (Some(spin), Some(indicator)) = (spin.upgrade(), indicator.upgrade()) else {
                    return;
                };
                indicator.hovered.set(true);
                spin.restyle(&indicator);
            }
        });
        motion.connect_leave({
            let spin = Rc::downgrade(self);
            let indicator = Rc::downgrade(indicator);
            move |_| {
                let (Some(spin), Some(indicator)) = (spin.upgrade(), indicator.upgrade()) else {
                    return;
                };
                indicator.hovered.set(false);
                spin.restyle(&indicator);
            }
        });
        indicator.area.add_controller(motion);

        let click = gtk4::GestureClick::new();
        click.connect_pressed({
            let spin = Rc::downgrade(self);
            let indicator = Rc::downgrade(indicator);
            move |_, _, _, _| {
                let (Some(spin), Some(indicator)) = (spin.upgrade(), indicator.upgrade()) else {
                    return;
                };
                indicator.pressed.set(true);
                indicator.repeated.set(false);
                spin.restyle(&indicator);
                let repeat = {
                    let spin = Rc::downgrade(&spin);
                    let indicator = indicator.clone();
                    move || {
                        let Some(spin) = spin.upgrade() else {
                            return glib::ControlFlow::Break;
                        };
                        indicator.repeated.set(true);
                        spin.commit(spin.value.get() + indicator.step * spin.step, true);
                        glib::ControlFlow::Continue
                    }
                };
                let slot = indicator.clone();
                let delay = glib::timeout_add_local_once(REPEAT_DELAY, move || {
                    repeat();
                    let every = glib::timeout_add_local(REPEAT_INTERVAL, repeat);
                    slot.timer.replace(Some(every));
                });
                indicator.timer.replace(Some(delay));
            }
        });
        let release = {
            let spin = Rc::downgrade(self);
            let indicator = Rc::downgrade(indicator);
            move |step: bool| {
                let Some(indicator) = indicator.upgrade() else {
                    return;
                };
                if let Some(timer) = indicator.timer.take() {
                    timer.remove();
                }
                let Some(spin) = spin.upgrade() else {
                    return;
                };
                indicator.pressed.set(false);
                spin.restyle(&indicator);
                if step && !indicator.repeated.get() {
                    spin.commit(spin.value.get() + indicator.step * spin.step, true);
                }
            }
        };
        let released = release.clone();
        click.connect_released(move |_, _, _, _| released(true));
        click.connect_cancel(move |_, _| release(false));
        indicator.area.add_controller(click);
    }

    fn restyle(&self, indicator: &Indicator) {
        let theme = self.theme.borrow();
        let target = if indicator.pressed.get() {
            theme.colors.col_layer2_active
        } else if indicator.hovered.get() {
            theme.colors.col_layer2_hover
        } else {
            transparentize(theme.colors.col_layer2, 1.0)
        };
        let now = self
            .background
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        indicator
            .fade
            .retarget(target, now, self.background.is_mapped());
        self.background.add_tick_callback({
            let down = self.down.clone();
            let up = self.up.clone();
            move |background, clock| {
                background.queue_draw();
                let now = clock.frame_time();
                if down.fade.running(now) || up.fade.running(now) {
                    return glib::ControlFlow::Continue;
                }
                glib::ControlFlow::Break
            }
        });
    }

    fn draw(&self, snapshot: &gtk4::Snapshot, width: f32, height: f32) {
        let theme = self.theme.borrow();
        let outer = rounding::SMALL as f32;
        let inner = rounding::UNSHARPEN as f32;
        let bounds = graphene::Rect::new(0.0, 0.0, width, height);
        rounded(snapshot, bounds, [outer; 4], theme.colors.col_layer2);
        let now = self
            .background
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let side = HEIGHT as f32;
        let top = (height - side) / 2.0;
        if let Some(colour) = self.down.fade.value(now) {
            let rect = graphene::Rect::new(0.0, top, side, side);
            rounded(snapshot, rect, [outer, inner, inner, outer], colour);
        }
        if let Some(colour) = self.up.fade.value(now) {
            let rect = graphene::Rect::new(width - side, top, side, side);
            rounded(snapshot, rect, [inner, outer, outer, inner], colour);
        }
    }
}

impl Indicator {
    fn new(icon: &str, step: i64) -> Rc<Self> {
        let symbol = text::symbol(icon, ICON);
        text::set_color(&symbol, "colOnLayer2");
        let area = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        area.set_size_request(HEIGHT, HEIGHT);
        let centred = Centred::integral(&symbol);
        centred.set_hexpand(true);
        area.append(&centred);
        pointer_cursor(area.upcast_ref());
        Rc::new(Indicator {
            area,
            step,
            fade: anim::Fade::new(),
            hovered: Cell::new(false),
            pressed: Cell::new(false),
            repeated: Cell::new(false),
            timer: RefCell::new(None),
        })
    }
}

fn rounded(snapshot: &gtk4::Snapshot, bounds: graphene::Rect, radii: [f32; 4], colour: RGBA) {
    let corner = |radius: f32| graphene::Size::new(radius, radius);
    let shape = gsk::RoundedRect::new(
        bounds,
        corner(radii[0]),
        corner(radii[1]),
        corner(radii[2]),
        corner(radii[3]),
    );
    snapshot.push_rounded_clip(&shape);
    snapshot.append_color(&colour, &bounds);
    snapshot.pop();
}
