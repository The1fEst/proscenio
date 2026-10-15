use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, Theme, pixel_size, rounding, transparentize};
use crate::ui::widgets::display::customicon;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::column::Column;
use crate::ui::widgets::layout::group::{GroupButton, Look as ButtonLook};
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

pub const CELL_HEIGHT: f64 = 56.0;
const PADDING: f64 = 6.0;
const ICON_SPAN: f64 = CELL_HEIGHT - PADDING * 2.0;
const MOVE_MILLIS: f64 = 500.0;
const FAST_MILLIS: f64 = 200.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub name: &'static str,
    pub status: String,
    pub has_status: bool,
    pub icon: String,
    pub tooltip: String,
    pub is_toggled: bool,
    pub is_available: bool,
}

pub enum Glyph {
    Symbol,
    Custom(&'static str),
}

struct Disc {
    area: gtk4::DrawingArea,
    radius: Cell<Tween>,
    color: Cell<Option<(RGBA, RGBA, i64)>>,
    layer: Cell<Option<(RGBA, RGBA, i64)>>,
    is_hovered: Cell<bool>,
    is_pressed: Cell<bool>,
    is_ticking: Cell<bool>,
}

pub struct QuickToggle {
    pub button: GroupButton,
    theme: SharedTheme,
    is_expanded: bool,
    has_alt: bool,
    symbol: Option<gtk4::Label>,
    custom: Option<gtk4::DrawingArea>,
    fill: Option<Rc<dyn Fn(f64)>>,
    name: Option<gtk4::Label>,
    status: Option<gtk4::Label>,
    disc: Option<Rc<Disc>>,
    tooltip: Rc<Tooltip>,
    tooltip_text: RefCell<String>,
    is_toggled: Cell<bool>,
    is_enabled: Cell<bool>,
    is_editing: Cell<bool>,
    last_look: RefCell<Option<Look>>,
    content: gtk4::Widget,
    main_action: RefCell<Option<Rc<dyn Fn()>>>,
    pub size: i32,
}

pub struct Start {
    pub slide_from: Option<f64>,
    pub should_fade_in: bool,
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
        let is_expanded = size > 1;
        let is_split = has_alt && is_expanded;
        let button = GroupButton::new(theme, width, CELL_HEIGHT);
        if let Some(from) = start.slide_from {
            button.slide_base_width(from, width);
        }
        button.set_size_animation(true, true);
        button.set_look(ButtonLook {
            background: |theme| theme.colors.col_layer2,
            toggled: if is_split {
                |theme: &Theme| theme.colors.col_layer2
            } else {
                |theme: &Theme| theme.colors.col_primary
            },
            toggled_hover: if is_split {
                |theme: &Theme| theme.colors.col_layer2_hover
            } else {
                |theme: &Theme| theme.colors.col_primary_hover
            },
            toggled_active: if is_split {
                |theme: &Theme| theme.colors.col_layer2_active
            } else {
                |theme: &Theme| theme.colors.col_primary_active
            },
            ..ButtonLook::default()
        });
        button.set_radii(CELL_HEIGHT / 2.0, rounding::NORMAL as f64);
        button.jump_radius();

        let icon_size = if is_expanded { 22.0 } else { 24.0 };
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
                    Centered::integral(&label).upcast(),
                )
            }
            Glyph::Custom(name) => {
                let image = customicon::build(name, if is_expanded { 24 } else { 27 });
                (
                    None,
                    Some(image.clone()),
                    None,
                    Centered::integral(&image).upcast(),
                )
            }
        };

        let mut disc_parts = None;
        let (name, status) = if is_expanded {
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
            if is_split {
                holder.set_cursor_from_name(Some("pointer"));
            }

            let disc = Rc::new(Disc {
                area: area.clone(),
                radius: Cell::new(Tween::new(
                    CELL_HEIGHT / 2.0 - PADDING,
                    MOVE_MILLIS,
                    EXPRESSIVE_DEFAULT,
                )),
                color: Cell::new(None),
                layer: Cell::new(None),
                is_hovered: Cell::new(false),
                is_pressed: Cell::new(false),
                is_ticking: Cell::new(false),
            });
            area.set_draw_func({
                let disc = Rc::downgrade(&disc);
                move |area, cr, width, height| {
                    if let Some(disc) = disc.upgrade() {
                        disc.draw(area, cr, width as f64, height as f64);
                    }
                }
            });
            disc_parts = Some((disc, holder.clone()));

            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            row.set_margin_start(PADDING as i32);
            row.set_margin_end(PADDING as i32);
            row.append(&holder);

            let name = text::styled_sized("", pixel_size::SMALLIE);
            name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            name.set_xalign(0.0);
            let status = text::styled_sized("", pixel_size::SMALLER);
            status.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            status.set_xalign(0.0);
            let column = Column::filling_width(-2);
            column.append(&name);
            column.append(&status);
            let text_holder = Centered::filling_width(&column);
            text_holder.set_hexpand(true);
            row.append(&text_holder);
            button.set_content(&row);
            (Some(name), Some(status))
        } else {
            button.set_content(&glyph_widget);
            (None, None)
        };

        let tooltip = Tooltip::new(&button, theme, tooltip::Kind::Styled);
        tooltip.place_like_qt();

        if start.should_fade_in {
            fade_in(&button);
        }

        let toggle = Rc::new(QuickToggle {
            button: button.clone(),
            theme: theme.clone(),
            is_expanded,
            has_alt,
            symbol,
            custom,
            fill,
            name,
            status,
            disc: disc_parts.as_ref().map(|(disc, _)| disc.clone()),
            tooltip: tooltip.clone(),
            tooltip_text: RefCell::new(String::new()),
            is_toggled: Cell::new(false),
            is_enabled: Cell::new(true),
            is_editing: Cell::new(false),
            last_look: RefCell::new(None),
            content: button.content().unwrap_or_else(|| button.clone().upcast()),
            main_action: RefCell::new(None),
            size,
        });

        let hover = gtk4::EventControllerMotion::new();
        hover.connect_enter({
            let tooltip = tooltip.clone();
            let toggle = Rc::downgrade(&toggle);
            move |_, _, _| {
                if toggle.upgrade().is_some_and(|toggle| {
                    toggle.is_enabled.get() && !toggle.is_tooltip_text_empty()
                }) {
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

        if let Some((disc, holder)) = disc_parts
            && is_split
        {
            let motion_controller = gtk4::EventControllerMotion::new();
            motion_controller.connect_enter({
                let disc = disc.clone();
                let toggle = Rc::downgrade(&toggle);
                move |_, _, _| {
                    disc.is_hovered.set(true);
                    if let Some(toggle) = toggle.upgrade() {
                        toggle.restyle_disc();
                    }
                }
            });
            motion_controller.connect_leave({
                let disc = disc.clone();
                let toggle = Rc::downgrade(&toggle);
                move |_| {
                    disc.is_hovered.set(false);
                    disc.is_pressed.set(false);
                    if let Some(toggle) = toggle.upgrade() {
                        toggle.restyle_disc();
                    }
                }
            });
            holder.add_controller(motion_controller);

            let click = gtk4::GestureClick::new();
            click.set_button(gtk4::gdk::BUTTON_PRIMARY);
            click.connect_pressed({
                let disc = disc.clone();
                let toggle = Rc::downgrade(&toggle);
                move |gesture, _, _, _| {
                    let Some(toggle) = toggle.upgrade() else {
                        return;
                    };
                    if !toggle.is_enabled.get() {
                        gesture.set_state(gtk4::EventSequenceState::Denied);
                        return;
                    }
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    disc.is_pressed.set(true);
                    toggle.restyle_disc();
                }
            });
            click.connect_released({
                let disc = disc.clone();
                let toggle = Rc::downgrade(&toggle);
                move |_, _, x, y| {
                    disc.is_pressed.set(false);
                    let Some(toggle) = toggle.upgrade() else {
                        return;
                    };
                    toggle.restyle_disc();
                    let is_inside = x >= 0.0 && y >= 0.0 && x < ICON_SPAN && y < ICON_SPAN;
                    let action = toggle.main_action.borrow().clone();
                    if let (true, Some(action)) = (is_inside, action) {
                        action();
                    }
                }
            });
            holder.add_controller(click);
        }

        toggle
    }

    fn is_tooltip_text_empty(&self) -> bool {
        self.tooltip_text.borrow().is_empty()
    }

    pub fn connect_actions(
        self: &Rc<Self>,
        main_action: Rc<dyn Fn()>,
        alt_action: Option<Rc<dyn Fn()>>,
    ) {
        self.main_action.replace(Some(main_action.clone()));
        let is_expanded = self.is_expanded;
        let click_alt = alt_action.clone();
        self.button
            .connect_clicked(move || match (&click_alt, is_expanded) {
                (Some(alt_action), true) => alt_action(),
                _ => main_action(),
            });
        if let Some(alt_action) = alt_action {
            self.button.connect_alt(move || alt_action());
        }
    }

    pub fn set_editing(&self, is_editing: bool) {
        self.is_editing.set(is_editing);
        self.button.set_size_animation(!is_editing, true);
        let last_look = self.last_look.borrow().clone();
        if let Some(look) = last_look {
            self.show(&look);
        }
    }

    pub fn set_dragged(&self, is_dragged: bool) {
        self.content
            .set_opacity(if is_dragged { 0.35 } else { 1.0 });
    }

    pub fn show(&self, look: &Look) {
        self.last_look.replace(Some(look.clone()));
        let is_enabled = look.is_available || self.is_editing.get();
        self.is_enabled.set(is_enabled);
        self.button.set_enabled(is_enabled);
        self.is_toggled.set(look.is_toggled);
        self.button.set_toggled(look.is_toggled);
        self.button.set_radii(
            if look.is_toggled {
                rounding::LARGE as f64
            } else {
                CELL_HEIGHT / 2.0
            },
            rounding::NORMAL as f64,
        );
        self.tooltip_text.replace(look.tooltip.clone());
        self.tooltip.set_text(&look.tooltip);

        let is_split = self.has_alt && self.is_expanded;
        let text_token = if look.is_toggled && !is_split && is_enabled {
            "colOnPrimary"
        } else {
            "colOnLayer2"
        };
        let icon_token = if self.is_expanded {
            if look.is_toggled {
                "colOnPrimary"
            } else {
                "colOnLayer3"
            }
        } else {
            text_token
        };
        let opacity = if is_enabled { 1.0 } else { 0.3 };

        if let Some(symbol) = &self.symbol {
            symbol.set_text(&look.icon);
            text::set_color(symbol, icon_token);
            symbol.set_opacity(if self.is_expanded { 1.0 } else { opacity });
        }
        if let Some(fill) = &self.fill {
            fill(if look.is_toggled { 1.0 } else { 0.0 });
        }
        if let Some(image) = &self.custom {
            text::set_color(image, icon_token);
            image.set_opacity(if self.is_expanded { 1.0 } else { opacity });
        }
        if let Some(name) = &self.name {
            name.set_text(&tr(look.name));
            text::set_color(name, text_token);
            name.set_opacity(opacity);
        }
        if let Some(status) = &self.status {
            let text = if !look.has_status {
                String::new()
            } else if look.status.is_empty() {
                tr(if look.is_toggled { "On" } else { "Off" })
            } else {
                look.status.clone()
            };
            status.set_visible(!text.is_empty());
            status.set_text(&text);
            text::set_color(status, text_token);
            status.set_opacity(opacity);
        }
        self.restyle_disc();
    }

    fn restyle_disc(&self) {
        let Some(disc) = &self.disc else {
            return;
        };
        let theme = self.theme.borrow();
        let now = disc.now();
        let is_split = self.has_alt && self.is_expanded;
        let is_toggled = self.is_toggled.get();
        let base = if is_toggled {
            theme.colors.col_primary
        } else {
            theme.colors.col_layer3
        };
        let color = transparentize(base, if is_split { 0.0 } else { 1.0 });
        let icon = if is_toggled {
            theme.colors.col_on_primary
        } else {
            theme.colors.col_on_layer3
        };
        let layer = if !is_split {
            transparentize(icon, 1.0)
        } else if disc.is_pressed.get() {
            transparentize(icon, 0.88)
        } else if disc.is_hovered.get() {
            transparentize(icon, 0.95)
        } else {
            transparentize(icon, 1.0)
        };
        let should_animate = disc.area.is_mapped();
        disc.retarget_color(&disc.color, color, now, should_animate);
        disc.retarget_color(&disc.layer, layer, now, should_animate);
        let mut radius = disc.radius.get();
        let target = self.button.radius() - PADDING;
        if should_animate {
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
        if current.is_running(now) {
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

    fn retarget_color(
        &self,
        slot: &Cell<Option<(RGBA, RGBA, i64)>>,
        target: RGBA,
        now: i64,
        should_animate: bool,
    ) {
        let current = match slot.get() {
            Some(state) => Self::blend(state, now),
            None => target,
        };
        if slot.get().is_some_and(|(_, to, _)| to == target) {
            return;
        }
        slot.set(Some(if should_animate {
            (current, target, now)
        } else {
            (target, target, now)
        }));
    }

    fn blend((from, to, start): (RGBA, RGBA, i64), now: i64) -> RGBA {
        let part = ((now - start) as f64 / (FAST_MILLIS * 1000.0)).clamp(0.0, 1.0);
        let eased_progress = EXPRESSIVE_EFFECTS.at(part) as f32;
        RGBA::new(
            from.red() + (to.red() - from.red()) * eased_progress,
            from.green() + (to.green() - from.green()) * eased_progress,
            from.blue() + (to.blue() - from.blue()) * eased_progress,
            from.alpha() + (to.alpha() - from.alpha()) * eased_progress,
        )
    }

    fn is_running(slot: &Cell<Option<(RGBA, RGBA, i64)>>, now: i64) -> bool {
        slot.get().is_some_and(|(from, to, start)| {
            from != to && ((now - start) as f64) < FAST_MILLIS * 1000.0
        })
    }

    fn tick(self: &Rc<Self>) {
        self.area.queue_draw();
        if self.is_ticking.replace(true) {
            return;
        }
        let disc = Rc::downgrade(self);
        self.area.add_tick_callback(move |area, clock| {
            area.queue_draw();
            let Some(disc) = disc.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            if disc.radius.get().is_running(now)
                || Self::is_running(&disc.color, now)
                || Self::is_running(&disc.layer, now)
            {
                return glib::ControlFlow::Continue;
            }
            disc.is_ticking.set(false);
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
        for slot in [&self.color, &self.layer] {
            let Some(state) = slot.get() else {
                continue;
            };
            let color = Self::blend(state, now);
            if color.alpha() <= 0.0 {
                continue;
            }
            cr.set_source_rgba(
                color.red() as f64,
                color.green() as f64,
                color.blue() as f64,
                color.alpha() as f64,
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
