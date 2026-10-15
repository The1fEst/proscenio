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
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::group::pointer_cursor;

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
    is_hovered: Cell<bool>,
    is_pressed: Cell<bool>,
    has_repeated: Cell<bool>,
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
    on_changed: RefCell<Option<Rc<dyn Fn(i64)>>>,
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
        let value_box = Centered::integral(&field);
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
            on_changed: RefCell::new(None),
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
        let focus_controller = gtk4::EventControllerFocus::new();
        focus_controller.connect_leave({
            let spin = Rc::downgrade(&spin);
            move |_| {
                if let Some(spin) = spin.upgrade() {
                    spin.finish_typing();
                }
            }
        });
        field.add_controller(focus_controller);
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.connect_key_pressed({
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
        field.add_controller(key_controller);
        spin.show_value();
        spin
    }

    pub fn connect_changed(&self, action: impl Fn(i64) + 'static) {
        self.on_changed.replace(Some(Rc::new(action)));
    }

    pub fn set_value(&self, value: i64) {
        let value = value.clamp(self.from, self.to);
        if self.value.replace(value) != value {
            self.show_value();
        }
    }

    pub fn set_enabled(&self, is_enabled: bool) {
        self.root.set_sensitive(is_enabled);
        self.root.set_opacity(if is_enabled { 1.0 } else { 0.4 });
    }

    fn factor(&self) -> i64 {
        10_i64.pow(self.decimals)
    }

    fn commit(&self, value: i64, should_show_value: bool) {
        let value = value.clamp(self.from, self.to);
        if self.value.replace(value) == value {
            return;
        }
        if should_show_value {
            self.show_value();
        }
        let on_changed = self.on_changed.borrow().clone();
        if let Some(on_changed) = on_changed {
            on_changed(value);
        }
    }

    fn finish_typing(&self) {
        if let Ok(number) = self.field.text().trim().parse::<f64>()
            && number.is_finite()
        {
            self.commit((number * self.factor() as f64).round() as i64, false);
        }
        self.show_value();
    }

    fn show_value(&self) {
        let display_value = self.value.get() as f64 / self.factor() as f64;
        self.field
            .set_text(&format!("{display_value:.*}", self.decimals as usize));
    }

    fn wire(self: &Rc<Self>, indicator: &Rc<Indicator>) {
        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_enter({
            let spin = Rc::downgrade(self);
            let indicator = Rc::downgrade(indicator);
            move |_, _, _| {
                let (Some(spin), Some(indicator)) = (spin.upgrade(), indicator.upgrade()) else {
                    return;
                };
                indicator.is_hovered.set(true);
                spin.restyle(&indicator);
            }
        });
        motion_controller.connect_leave({
            let spin = Rc::downgrade(self);
            let indicator = Rc::downgrade(indicator);
            move |_| {
                let (Some(spin), Some(indicator)) = (spin.upgrade(), indicator.upgrade()) else {
                    return;
                };
                indicator.is_hovered.set(false);
                spin.restyle(&indicator);
            }
        });
        indicator.area.add_controller(motion_controller);

        let click = gtk4::GestureClick::new();
        click.connect_pressed({
            let spin = Rc::downgrade(self);
            let indicator = Rc::downgrade(indicator);
            move |_, _, _, _| {
                let (Some(spin), Some(indicator)) = (spin.upgrade(), indicator.upgrade()) else {
                    return;
                };
                indicator.is_pressed.set(true);
                indicator.has_repeated.set(false);
                spin.restyle(&indicator);
                let repeat = {
                    let spin = Rc::downgrade(&spin);
                    let indicator = indicator.clone();
                    move || {
                        let Some(spin) = spin.upgrade() else {
                            return glib::ControlFlow::Break;
                        };
                        indicator.has_repeated.set(true);
                        spin.commit(spin.value.get() + indicator.step * spin.step, true);
                        glib::ControlFlow::Continue
                    }
                };
                let repeat_indicator = indicator.clone();
                let delay_source = glib::timeout_add_local_once(REPEAT_DELAY, move || {
                    repeat();
                    let repeat_source = glib::timeout_add_local(REPEAT_INTERVAL, repeat);
                    repeat_indicator.timer.replace(Some(repeat_source));
                });
                indicator.timer.replace(Some(delay_source));
            }
        });
        let release = {
            let spin = Rc::downgrade(self);
            let indicator = Rc::downgrade(indicator);
            move |should_step: bool| {
                let Some(indicator) = indicator.upgrade() else {
                    return;
                };
                if let Some(timer) = indicator.timer.take() {
                    timer.remove();
                }
                let Some(spin) = spin.upgrade() else {
                    return;
                };
                indicator.is_pressed.set(false);
                spin.restyle(&indicator);
                if should_step && !indicator.has_repeated.get() {
                    spin.commit(spin.value.get() + indicator.step * spin.step, true);
                }
            }
        };
        let release_for_click = release.clone();
        click.connect_released(move |_, _, _, _| release_for_click(true));
        click.connect_cancel(move |_, _| release(false));
        indicator.area.add_controller(click);
    }

    fn restyle(&self, indicator: &Indicator) {
        let theme = self.theme.borrow();
        let target = if indicator.is_pressed.get() {
            theme.colors.col_layer2_active
        } else if indicator.is_hovered.get() {
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
                if down.fade.is_running(now) || up.fade.is_running(now) {
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
        append_rounded(snapshot, bounds, [outer; 4], theme.colors.col_layer2);
        let now = self
            .background
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let side = HEIGHT as f32;
        let top = (height - side) / 2.0;
        if let Some(color) = self.down.fade.value(now) {
            let rect = graphene::Rect::new(0.0, top, side, side);
            append_rounded(snapshot, rect, [outer, inner, inner, outer], color);
        }
        if let Some(color) = self.up.fade.value(now) {
            let rect = graphene::Rect::new(width - side, top, side, side);
            append_rounded(snapshot, rect, [inner, outer, outer, inner], color);
        }
    }
}

impl Indicator {
    fn new(icon: &str, step: i64) -> Rc<Self> {
        let symbol = text::symbol(icon, ICON);
        text::set_color(&symbol, "colOnLayer2");
        let area = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        area.set_size_request(HEIGHT, HEIGHT);
        let symbol_box = Centered::integral(&symbol);
        symbol_box.set_hexpand(true);
        area.append(&symbol_box);
        pointer_cursor(area.upcast_ref());
        Rc::new(Indicator {
            area,
            step,
            fade: anim::Fade::new(),
            is_hovered: Cell::new(false),
            is_pressed: Cell::new(false),
            has_repeated: Cell::new(false),
            timer: RefCell::new(None),
        })
    }
}

fn append_rounded(snapshot: &gtk4::Snapshot, bounds: graphene::Rect, radii: [f32; 4], color: RGBA) {
    let corner = |radius: f32| graphene::Size::new(radius, radius);
    let shape = gsk::RoundedRect::new(
        bounds,
        corner(radii[0]),
        corner(radii[1]),
        corner(radii[2]),
        corner(radii[3]),
    );
    snapshot.push_rounded_clip(&shape);
    snapshot.append_color(&color, &bounds);
    snapshot.pop();
}
