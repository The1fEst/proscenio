use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, Theme, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::column::Column;
use crate::ui::widgets::customicon;
use crate::ui::widgets::group::{GroupButton, Look as ButtonLook};
use crate::ui::widgets::text;
use crate::ui::widgets::tooltip::{self, Tooltip};

pub const CELL_HEIGHT: f64 = 56.0;
const PADDING: f64 = 6.0;
const ICON_SPAN: f64 = CELL_HEIGHT - PADDING * 2.0;
const MOVE_MILLIS: f64 = 500.0;
const FAST_MILLIS: f64 = 200.0;

#[derive(Clone)]
pub struct Look {
    pub name: &'static str,
    pub status: String,
    pub has_status: bool,
    pub icon: String,
    pub tooltip: String,
    pub toggled: bool,
    pub available: bool,
}

pub enum Glyph {
    Symbol,
    Custom(&'static str),
}

struct Disc {
    area: gtk4::DrawingArea,
    radius: Cell<Tween>,
    colour: Cell<Option<(RGBA, RGBA, i64)>>,
    layer: Cell<Option<(RGBA, RGBA, i64)>>,
    hovered: Cell<bool>,
    pressed: Cell<bool>,
    ticking: Cell<bool>,
}

pub struct QuickToggle {
    pub button: GroupButton,
    theme: SharedTheme,
    expanded: bool,
    has_alt: bool,
    symbol: Option<gtk4::Label>,
    custom: Option<gtk4::DrawingArea>,
    fill: Option<Rc<dyn Fn(f64)>>,
    name: Option<gtk4::Label>,
    status: Option<gtk4::Label>,
    disc: Option<Rc<Disc>>,
    tooltip: Rc<Tooltip>,
    tooltip_text: RefCell<String>,
    toggled: Cell<bool>,
    enabled: Cell<bool>,
    editing: Cell<bool>,
    last: RefCell<Option<Look>>,
    content: gtk4::Widget,
    main: RefCell<Option<Rc<dyn Fn()>>>,
    pub size: i32,
}

pub struct Start {
    pub slide_from: Option<f64>,
    pub fade_in: bool,
}

impl QuickToggle {
    pub fn new(
        theme: &SharedTheme,
        width: f64,
        size: i32,
        has_alt: bool,
        glyph: Glyph,
        start: Start,
    ) -> Rc<Self> {
        let expanded = size > 1;
        let split = has_alt && expanded;
        let button = GroupButton::new(theme, width, CELL_HEIGHT);
        if let Some(from) = start.slide_from {
            button.slide_base_width(from, width);
        }
        button.set_size_animation(true, true);
        button.set_look(ButtonLook {
            background: |theme| theme.colors.col_layer2,
            toggled: if split {
                |theme: &Theme| theme.colors.col_layer2
            } else {
                |theme: &Theme| theme.colors.col_primary
            },
            toggled_hover: if split {
                |theme: &Theme| theme.colors.col_layer2_hover
            } else {
                |theme: &Theme| theme.colors.col_primary_hover
            },
            toggled_active: if split {
                |theme: &Theme| theme.colors.col_layer2_active
            } else {
                |theme: &Theme| theme.colors.col_primary_active
            },
            ..ButtonLook::default()
        });
        button.set_radii(CELL_HEIGHT / 2.0, rounding::NORMAL as f64);
        button.jump_radius();

        let icon_size = if expanded { 22.0 } else { 24.0 };
        let (symbol, custom, fill, glyph_widget): (
            Option<gtk4::Label>,
            Option<gtk4::DrawingArea>,
            Option<Rc<dyn Fn(f64)>>,
            gtk4::Widget,
        ) = match glyph {
            Glyph::Symbol => {
                let label = text::symbol("", icon_size);
                let fill = text::fill_motion(&label, icon_size, 0.0);
                (
                    Some(label.clone()),
                    None,
                    Some(fill),
                    Centred::integral(&label).upcast(),
                )
            }
            Glyph::Custom(name) => {
                let image = customicon::build(name, if expanded { 24 } else { 27 });
                (
                    None,
                    Some(image.clone()),
                    None,
                    Centred::integral(&image).upcast(),
                )
            }
        };

        let with_text = expanded && symbol.is_some();
        let mut disc = None;
        let (name, status) = if expanded {
            let area = gtk4::DrawingArea::new();
            area.set_content_width(ICON_SPAN as i32);
            area.set_content_height(ICON_SPAN as i32);
            let holder = gtk4::Overlay::new();
            holder.set_child(Some(&area));
            holder.add_overlay(&glyph_widget);
            holder.set_margin_top(PADDING as i32);
            holder.set_margin_bottom(PADDING as i32);
            holder.set_size_request(ICON_SPAN as i32, ICON_SPAN as i32);
            holder.set_valign(gtk4::Align::Fill);
            if split {
                holder.set_cursor_from_name(Some("pointer"));
            }

            let made = Rc::new(Disc {
                area: area.clone(),
                radius: Cell::new(Tween::new(
                    CELL_HEIGHT / 2.0 - PADDING,
                    MOVE_MILLIS,
                    EXPRESSIVE_DEFAULT,
                )),
                colour: Cell::new(None),
                layer: Cell::new(None),
                hovered: Cell::new(false),
                pressed: Cell::new(false),
                ticking: Cell::new(false),
            });
            area.set_draw_func({
                let made = Rc::downgrade(&made);
                move |area, cr, width, height| {
                    if let Some(made) = made.upgrade() {
                        made.draw(area, cr, width as f64, height as f64);
                    }
                }
            });
            disc = Some((made, holder.clone()));

            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            row.set_margin_start(PADDING as i32);
            row.set_margin_end(PADDING as i32);
            row.append(&holder);

            if with_text {
                let name = text::styled_sized("", pixel_size::SMALLIE);
                name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                name.set_xalign(0.0);
                let status = text::styled_sized("", pixel_size::SMALLER);
                status.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                status.set_xalign(0.0);
                let column = Column::filling_width(-2);
                column.append(&name);
                column.append(&status);
                let centred = Centred::filling_width(&column);
                centred.set_hexpand(true);
                row.append(&centred);
                button.set_content(&row);
                (Some(name), Some(status))
            } else {
                button.set_content(&row);
                (None, None)
            }
        } else {
            button.set_content(&glyph_widget);
            (None, None)
        };

        let tooltip = Tooltip::new(&button, theme, tooltip::Kind::Styled);
        tooltip.place_like_qt();

        if start.fade_in {
            fade_in(&button);
        }

        let toggle = Rc::new(QuickToggle {
            button: button.clone(),
            theme: theme.clone(),
            expanded,
            has_alt,
            symbol,
            custom,
            fill,
            name,
            status,
            disc: disc.as_ref().map(|(made, _)| made.clone()),
            tooltip: tooltip.clone(),
            tooltip_text: RefCell::new(String::new()),
            toggled: Cell::new(false),
            enabled: Cell::new(true),
            editing: Cell::new(false),
            last: RefCell::new(None),
            content: button.content().unwrap_or_else(|| button.clone().upcast()),
            main: RefCell::new(None),
            size,
        });

        let hover = gtk4::EventControllerMotion::new();
        hover.connect_enter({
            let tooltip = tooltip.clone();
            let toggle = Rc::downgrade(&toggle);
            move |_, _, _| {
                if toggle
                    .upgrade()
                    .is_some_and(|toggle| toggle.enabled.get() && !toggle.tooltip_text_empty())
                {
                    tooltip.show(true);
                }
            }
        });
        hover.connect_leave(move |_| tooltip.show(false));
        button.add_controller(hover);

        button.connect_changed({
            let toggle = Rc::downgrade(&toggle);
            move || {
                if let Some(toggle) = toggle.upgrade() {
                    toggle.restyle_disc();
                }
            }
        });

        if let Some((made, holder)) = disc
            && split
        {
            let motion = gtk4::EventControllerMotion::new();
            motion.connect_enter({
                let made = made.clone();
                let toggle = Rc::downgrade(&toggle);
                move |_, _, _| {
                    made.hovered.set(true);
                    if let Some(toggle) = toggle.upgrade() {
                        toggle.restyle_disc();
                    }
                }
            });
            motion.connect_leave({
                let made = made.clone();
                let toggle = Rc::downgrade(&toggle);
                move |_| {
                    made.hovered.set(false);
                    made.pressed.set(false);
                    if let Some(toggle) = toggle.upgrade() {
                        toggle.restyle_disc();
                    }
                }
            });
            holder.add_controller(motion);

            let click = gtk4::GestureClick::new();
            click.set_button(gtk4::gdk::BUTTON_PRIMARY);
            click.connect_pressed({
                let made = made.clone();
                let toggle = Rc::downgrade(&toggle);
                move |gesture, _, _, _| {
                    let Some(toggle) = toggle.upgrade() else {
                        return;
                    };
                    if !toggle.enabled.get() {
                        gesture.set_state(gtk4::EventSequenceState::Denied);
                        return;
                    }
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    made.pressed.set(true);
                    toggle.restyle_disc();
                }
            });
            click.connect_released({
                let made = made.clone();
                let toggle = Rc::downgrade(&toggle);
                move |_, _, x, y| {
                    made.pressed.set(false);
                    let Some(toggle) = toggle.upgrade() else {
                        return;
                    };
                    toggle.restyle_disc();
                    let inside = x >= 0.0 && y >= 0.0 && x < ICON_SPAN && y < ICON_SPAN;
                    let action = toggle.main.borrow().clone();
                    if let (true, Some(action)) = (inside, action) {
                        action();
                    }
                }
            });
            holder.add_controller(click);
        }

        toggle
    }

    fn tooltip_text_empty(&self) -> bool {
        self.tooltip_text.borrow().is_empty()
    }

    pub fn connect_actions(self: &Rc<Self>, main: Rc<dyn Fn()>, alt: Option<Rc<dyn Fn()>>) {
        self.main.replace(Some(main.clone()));
        let expanded = self.expanded;
        let click_alt = alt.clone();
        self.button
            .connect_clicked(move || match (&click_alt, expanded) {
                (Some(alt), true) => alt(),
                _ => main(),
            });
        if let Some(alt) = alt {
            self.button.connect_alt(move || alt());
        }
    }

    pub fn set_editing(&self, editing: bool) {
        self.editing.set(editing);
        self.button.set_size_animation(!editing, true);
        let last = self.last.borrow().clone();
        if let Some(look) = last {
            self.show(&look);
        }
    }

    pub fn set_dragged(&self, dragged: bool) {
        self.content.set_opacity(if dragged { 0.35 } else { 1.0 });
    }

    pub fn show(&self, look: &Look) {
        self.last.replace(Some(look.clone()));
        let enabled = look.available || self.editing.get();
        self.enabled.set(enabled);
        self.button.set_enabled(enabled);
        self.toggled.set(look.toggled);
        self.button.set_toggled(look.toggled);
        self.button.set_radii(
            if look.toggled {
                rounding::LARGE as f64
            } else {
                CELL_HEIGHT / 2.0
            },
            rounding::NORMAL as f64,
        );
        self.tooltip_text.replace(look.tooltip.clone());
        self.tooltip.set_text(&look.tooltip);

        let split = self.has_alt && self.expanded;
        let text_token = if look.toggled && !split && enabled {
            "colOnPrimary"
        } else {
            "colOnLayer2"
        };
        let icon_token = if self.expanded {
            if look.toggled {
                "colOnPrimary"
            } else {
                "colOnLayer3"
            }
        } else {
            text_token
        };
        let faded = if enabled { 1.0 } else { 0.3 };

        if let Some(symbol) = &self.symbol {
            symbol.set_text(&look.icon);
            text::set_color(symbol, icon_token);
            symbol.set_opacity(if self.expanded { 1.0 } else { faded });
        }
        if let Some(fill) = &self.fill {
            fill(if look.toggled { 1.0 } else { 0.0 });
        }
        if let Some(image) = &self.custom {
            text::set_color(image, icon_token);
            image.set_opacity(if self.expanded { 1.0 } else { faded });
        }
        if let Some(name) = &self.name {
            name.set_text(look.name);
            text::set_color(name, text_token);
            name.set_opacity(faded);
        }
        if let Some(status) = &self.status {
            let text = if !look.has_status {
                String::new()
            } else if look.status.is_empty() {
                if look.toggled { "On" } else { "Off" }.to_owned()
            } else {
                look.status.clone()
            };
            status.set_visible(!text.is_empty());
            status.set_text(&text);
            text::set_color(status, text_token);
            status.set_opacity(faded);
        }
        self.restyle_disc();
    }

    fn restyle_disc(&self) {
        let Some(disc) = &self.disc else {
            return;
        };
        let theme = self.theme.borrow();
        let now = disc.now();
        let split = self.has_alt && self.expanded;
        let toggled = self.toggled.get();
        let base = if toggled {
            theme.colors.col_primary
        } else {
            theme.colors.col_layer3
        };
        let colour = transparentize(base, if split { 0.0 } else { 1.0 });
        let icon = if toggled {
            theme.colors.col_on_primary
        } else {
            theme.colors.col_on_layer3
        };
        let layer = if !split {
            transparentize(icon, 1.0)
        } else if disc.pressed.get() {
            transparentize(icon, 0.88)
        } else if disc.hovered.get() {
            transparentize(icon, 0.95)
        } else {
            transparentize(icon, 1.0)
        };
        let animate = disc.area.is_mapped();
        disc.retarget_colour(&disc.colour, colour, now, animate);
        disc.retarget_colour(&disc.layer, layer, now, animate);
        let mut radius = disc.radius.get();
        let target = self.button.radius() - PADDING;
        if animate {
            radius.retarget(target, now);
        } else {
            radius.jump(target);
        }
        disc.radius.set(radius);
        disc.tick();
    }
}

fn fade_in(button: &GroupButton) {
    button.set_opacity(0.0);
    let tween = Cell::new(Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS));
    button.add_tick_callback(move |button, clock| {
        let now = clock.frame_time();
        let mut current = tween.get();
        if current.target() < 1.0 {
            current.retarget(1.0, now);
            tween.set(current);
        }
        button.set_opacity(current.value(now));
        if current.running(now) {
            return glib::ControlFlow::Continue;
        }
        button.set_opacity(1.0);
        glib::ControlFlow::Break
    });
}

impl Disc {
    fn now(&self) -> i64 {
        self.area
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    fn retarget_colour(
        &self,
        slot: &Cell<Option<(RGBA, RGBA, i64)>>,
        target: RGBA,
        now: i64,
        animate: bool,
    ) {
        let current = match slot.get() {
            Some(state) => Self::blend(state, now),
            None => target,
        };
        if slot.get().is_some_and(|(_, to, _)| to == target) {
            return;
        }
        slot.set(Some(if animate {
            (current, target, now)
        } else {
            (target, target, now)
        }));
    }

    fn blend((from, to, start): (RGBA, RGBA, i64), now: i64) -> RGBA {
        let part = ((now - start) as f64 / (FAST_MILLIS * 1000.0)).clamp(0.0, 1.0);
        let eased = EXPRESSIVE_EFFECTS.at(part) as f32;
        RGBA::new(
            from.red() + (to.red() - from.red()) * eased,
            from.green() + (to.green() - from.green()) * eased,
            from.blue() + (to.blue() - from.blue()) * eased,
            from.alpha() + (to.alpha() - from.alpha()) * eased,
        )
    }

    fn running(slot: &Cell<Option<(RGBA, RGBA, i64)>>, now: i64) -> bool {
        slot.get().is_some_and(|(from, to, start)| {
            from != to && ((now - start) as f64) < FAST_MILLIS * 1000.0
        })
    }

    fn tick(self: &Rc<Self>) {
        self.area.queue_draw();
        if self.ticking.replace(true) {
            return;
        }
        let disc = Rc::downgrade(self);
        self.area.add_tick_callback(move |area, clock| {
            area.queue_draw();
            let Some(disc) = disc.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            if disc.radius.get().running(now)
                || Self::running(&disc.colour, now)
                || Self::running(&disc.layer, now)
            {
                return glib::ControlFlow::Continue;
            }
            disc.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn draw(&self, _area: &gtk4::DrawingArea, cr: &gtk4::cairo::Context, width: f64, height: f64) {
        let now = self.now();
        let radius = self
            .radius
            .get()
            .value(now)
            .clamp(0.0, width.min(height) / 2.0);
        for slot in [&self.colour, &self.layer] {
            let Some(state) = slot.get() else {
                continue;
            };
            let colour = Self::blend(state, now);
            if colour.alpha() <= 0.0 {
                continue;
            }
            cr.set_source_rgba(
                colour.red() as f64,
                colour.green() as f64,
                colour.blue() as f64,
                colour.alpha() as f64,
            );
            cr.new_sub_path();
            cr.arc(width - radius, radius, radius, -PI / 2.0, 0.0);
            cr.arc(width - radius, height - radius, radius, 0.0, PI / 2.0);
            cr.arc(radius, height - radius, radius, PI / 2.0, PI);
            cr.arc(radius, radius, radius, PI, 1.5 * PI);
            cr.close_path();
            let _ = cr.fill();
        }
    }
}
