use gtk4::cairo;
use gtk4::gdk::RGBA;
use gtk4::graphene;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::platform::appicon;
use crate::platform::hypr;
use crate::services::hyprstate::Snapshot;
use crate::ui::anim;
use crate::ui::theme::{SharedTheme, transparentize};
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::text;

const BUTTON: f64 = 26.0;
const ACTIVE_MARGIN: f64 = 2.0;
const ACTIVE_SIZE: f64 = BUTTON - ACTIVE_MARGIN * 2.0;
const DOT: f64 = BUTTON * 0.18;
const BAR: f64 = crate::core::config::BASE_BAR_HEIGHT as f64;
const DRAG_LAYER: f64 = 0.16;
const HOVER_LAYER: f64 = 0.08;
const PRESS_LAYER: f64 = 0.1;
const ICON_SPAN: f64 = BUTTON * 0.69;
const ICON_SPAN_SHRUNK: f64 = BUTTON * 0.55;
const ICON_SIZE: i32 = 18;
const ICON_MARGIN_SHRUNK: f64 = -4.0;
const NUMBER_SIZE: f64 = 15.0;
const SPECIAL_TEXT_SIZE: f64 = BUTTON * 0.5;
const SPECIAL_VERTICAL_SHARE: f64 = 1.35;
const BLUR_MAX: f64 = 32.0;

#[derive(Default)]
struct State {
    active: i32,
    occupied: HashSet<i32>,
    classes: HashMap<i32, String>,
    special: String,
    super_held: bool,
}

struct Look {
    shown: i32,
    show_app_icons: bool,
    always_show_numbers: bool,
    monochrome_icons: bool,
    number_map: Vec<String>,
    font: text::Family,
    vertical: bool,
}

impl Look {
    fn place(&self, along: f64, across: f64, length: f64, thickness: f64) -> (f64, f64, f64, f64) {
        if self.vertical {
            (across, along, thickness, length)
        } else {
            (along, across, length, thickness)
        }
    }

    fn point(&self, along: f64, across: f64) -> (f64, f64) {
        if self.vertical {
            (across, along)
        } else {
            (along, across)
        }
    }
}

pub fn build(
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &crate::services::Services,
    monitor: &str,
    toggle_overview: impl Fn() + 'static,
    scope: &Scope,
) -> gtk4::Widget {
    let shown = config.workspaces_shown;
    let look = Rc::new(Look {
        shown,
        show_app_icons: config.workspaces_show_app_icons,
        always_show_numbers: config.workspaces_always_show_numbers,
        monochrome_icons: config.workspaces_monochrome_icons,
        number_map: config.workspaces_number_map.clone(),
        font: if config.workspaces_use_nerd_font {
            text::Family::Nerd
        } else {
            text::Family::Main
        },
        vertical: config.vertical,
    });
    let vertical = look.vertical;

    let area = Paint::new(|_, _, _| {});
    if vertical {
        area.set_size_request(BUTTON as i32, (BUTTON * shown as f64) as i32);
    } else {
        area.set_size_request((BUTTON * shown as f64) as i32, BUTTON as i32);
        area.set_valign(gtk4::Align::Center);
    }
    let along = move |x: f64, y: f64| if vertical { y } else { x };

    let state = Rc::new(RefCell::new(State {
        active: 1,
        ..State::default()
    }));
    refresh(&state, &services.hypr.snapshot(), monitor);

    let motion = Rc::new(Motion::new(&area, &look, &state.borrow()));
    motion.settle(&state.borrow(), &look);

    area.set_draw({
        let state = state.clone();
        let theme = theme.clone();
        let motion = motion.clone();
        let look = look.clone();
        let canvas = area.downgrade();
        move |snapshot, width, height| {
            let Some(canvas) = canvas.upgrade() else {
                return;
            };
            let bleed = BLUR_MAX as f32;
            let bounds =
                graphene::Rect::new(-bleed, -bleed, width + 2.0 * bleed, height + 2.0 * bleed);
            let cr = snapshot.append_cairo(&bounds);
            draw(
                &cr,
                &canvas,
                width as i32,
                height as i32,
                &look,
                &state.borrow(),
                &motion,
                &theme.borrow(),
            );
        }
    });

    let click = gtk4::GestureClick::new();
    click.set_button(gtk4::gdk::BUTTON_PRIMARY);
    click.connect_pressed({
        let state = state.clone();
        let motion = motion.clone();
        move |gesture, _, x, y| {
            motion.pressed.set(true);
            if let Some(area) = gesture.widget() {
                area.queue_draw();
            }
            let state = state.borrow();
            let index = (along(x, y) / BUTTON).floor() as i32;
            if !(0..shown).contains(&index) {
                return;
            }
            let group = (state.active - 1).div_euclid(shown);
            focus(&(group * shown + index + 1).to_string());
        }
    });
    click.connect_released({
        let motion = motion.clone();
        move |gesture, _, _, _| {
            motion.pressed.set(false);
            if let Some(area) = gesture.widget() {
                area.queue_draw();
            }
        }
    });
    area.add_controller(click);

    let back = gtk4::GestureClick::new();
    back.set_button(8);
    back.connect_pressed(|_, _, _, _| {
        hypr::request("dispatch hl.dsp.workspace.toggle_special(\"special\")");
    });
    area.add_controller(back);

    let secondary = gtk4::GestureClick::new();
    secondary.set_button(gtk4::gdk::BUTTON_SECONDARY);
    secondary.connect_pressed(move |_, _, _, _| toggle_overview());
    area.add_controller(secondary);

    let pointer = gtk4::EventControllerMotion::new();
    pointer.connect_motion({
        let motion = motion.clone();
        let state = state.clone();
        move |pointer, x, y| {
            motion.hovered.set(true);
            motion.touch(
                (along(x, y) / BUTTON)
                    .floor()
                    .clamp(0.0, (shown - 1) as f64),
            );
            motion.aim_special(&state.borrow());
            if let Some(area) = pointer.widget() {
                area.queue_draw();
            }
        }
    });
    pointer.connect_leave({
        let motion = motion.clone();
        let state = state.clone();
        move |pointer| {
            motion.hovered.set(false);
            motion.pressed.set(false);
            motion.touch((state.borrow().active - 1).rem_euclid(shown) as f64);
            motion.aim_special(&state.borrow());
            if let Some(area) = pointer.widget() {
                area.queue_draw();
            }
        }
    });
    area.add_controller(pointer);

    let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll(|_, _, dy| {
        if dy > 0.0 {
            focus("\"r+1\"");
        } else if dy < 0.0 {
            focus("\"r-1\"");
        }
        gtk4::glib::Propagation::Stop
    });
    area.add_controller(scroll);

    scope.keep(services.hypr.subscribe({
        let state = state.clone();
        let area = area.clone();
        let motion = motion.clone();
        let look = look.clone();
        let monitor = monitor.to_owned();
        let hypr = services.hypr.clone();
        move || {
            refresh(&state, &hypr.snapshot(), &monitor);
            motion.aim(&state.borrow(), &look);
            area.queue_draw();
        }
    }));

    if config.super_show {
        let pending: Rc<RefCell<Option<gtk4::glib::SourceId>>> = Rc::new(RefCell::new(None));
        let delay = std::time::Duration::from_millis(config.super_show_delay as u64);
        let states = services.states.clone();
        let held = {
            let state = state.clone();
            let motion = motion.clone();
            let look = look.clone();
            let area = area.clone();
            move |held: bool| {
                state.borrow_mut().super_held = held;
                motion.aim(&state.borrow(), &look);
                area.queue_draw();
            }
        };
        let held = Rc::new(held);
        scope.keep(services.states.subscribe(move || {
            if let Some(timer) = pending.borrow_mut().take() {
                timer.remove();
            }
            if states.super_down.get() {
                let held = held.clone();
                let slot = pending.clone();
                pending.replace(Some(gtk4::glib::timeout_add_local_once(delay, move || {
                    slot.replace(None);
                    held(true);
                })));
            } else {
                held(false);
            }
        }));
    }

    area.upcast()
}

struct Motion {
    lead: Rc<anim::Motion>,
    trail: Rc<anim::Motion>,
    touch_lead: Rc<anim::Motion>,
    touch_trail: Rc<anim::Motion>,
    hovered: Cell<bool>,
    pressed: Cell<bool>,
    thickness: Vec<Rc<anim::Motion>>,
    length: Vec<Rc<anim::Motion>>,
    offset: Vec<Rc<anim::Motion>>,
    numbers: Vec<Rc<anim::Motion>>,
    icon_alpha: Vec<Rc<anim::Motion>>,
    icon_margin: Vec<Rc<anim::Motion>>,
    icon_scale: Vec<Rc<anim::Motion>>,
    special: Rc<anim::Motion>,
    special_length: Rc<anim::Motion>,
}

impl Motion {
    fn new(area: &Paint, look: &Look, state: &State) -> Self {
        let shown = look.shown;
        let index = (state.active - 1).rem_euclid(shown) as f64;
        let cells = |start: f64, millis: f64, ease: anim::Ease| {
            (0..shown)
                .map(|_| anim::Motion::new(area, start, millis, ease))
                .collect::<Vec<_>>()
        };
        Motion {
            lead: anim::Motion::new(area, index, 100.0, anim::Ease::OutSine),
            trail: anim::Motion::new(area, index, 300.0, anim::Ease::OutSine),
            touch_lead: anim::Motion::new(area, index, 100.0, anim::Ease::OutSine),
            touch_trail: anim::Motion::new(area, index, 300.0, anim::Ease::OutSine),
            hovered: Cell::new(false),
            pressed: Cell::new(false),
            thickness: cells(0.0, 350.0, anim::EXPRESSIVE_FAST),
            length: cells(0.0, 350.0, anim::EXPRESSIVE_FAST),
            offset: cells(0.0, 350.0, anim::EXPRESSIVE_FAST),
            numbers: cells(0.0, 200.0, anim::EXPRESSIVE_EFFECTS),
            icon_alpha: cells(0.0, 200.0, anim::EXPRESSIVE_EFFECTS),
            icon_margin: cells(ICON_MARGIN_SHRUNK, 350.0, anim::EXPRESSIVE_FAST),
            icon_scale: cells(1.0, 350.0, anim::EXPRESSIVE_FAST),
            special: anim::Motion::new(area, 0.0, 350.0, anim::EXPRESSIVE_FAST),
            special_length: anim::Motion::new(area, 0.0, 400.0, anim::EMPHASIZED_DECEL),
        }
    }

    fn touch(&self, index: f64) {
        self.touch_lead.to(index);
        self.touch_trail.to(index);
    }

    fn targets(state: &State, shown: i32, index: i32) -> (f64, f64, f64) {
        let group = (state.active - 1).div_euclid(shown);
        let occupied = |at: i32| {
            (0..shown).contains(&at) && state.occupied.contains(&(group * shown + at + 1))
        };
        let here = occupied(index) as i32 as f64;
        let previous = occupied(index - 1) as i32 as f64;
        let next = occupied(index + 1) as i32 as f64;
        (
            BUTTON * here,
            BUTTON * (1.0 + 0.5 * previous + 0.5 * next) * here,
            (if here == 0.0 { 0.5 } else { -0.5 * previous }) * BUTTON,
        )
    }

    fn marks(state: &State, look: &Look, index: i32) -> (f64, f64, f64, f64) {
        let group = (state.active - 1).div_euclid(look.shown);
        let id = group * look.shown + index + 1;
        let window = state.classes.contains_key(&id);
        let numbers =
            state.super_held || look.always_show_numbers && (!look.show_app_icons || !window);
        let icon = look.show_app_icons && window;
        (
            numbers as i32 as f64,
            icon as i32 as f64,
            if icon && !state.super_held {
                (BUTTON - ICON_SPAN) / 2.0
            } else {
                ICON_MARGIN_SHRUNK
            },
            if look.show_app_icons && !state.super_held {
                1.0
            } else {
                ICON_SPAN_SHRUNK / ICON_SPAN
            },
        )
    }

    fn settle(&self, state: &State, look: &Look) {
        for index in 0..look.shown {
            let slot = index as usize;
            let (thickness, length, offset) = Self::targets(state, look.shown, index);
            self.thickness[slot].jump(thickness);
            self.length[slot].jump(length);
            self.offset[slot].jump(offset);
            let (numbers, icon, margin, scale) = Self::marks(state, look, index);
            self.numbers[slot].jump(numbers);
            self.icon_alpha[slot].jump(icon);
            self.icon_margin[slot].jump(margin);
            self.icon_scale[slot].jump(scale);
        }
        self.special
            .jump(if state.special.is_empty() { 0.0 } else { 1.0 });
    }

    fn aim_special(&self, state: &State) {
        let up = !state.special.is_empty() && !self.hovered.get();
        self.special.to(if up { 1.0 } else { 0.0 });
    }

    fn aim(&self, state: &State, look: &Look) {
        let index = (state.active - 1).rem_euclid(look.shown) as f64;
        self.lead.to(index);
        self.trail.to(index);
        if !self.hovered.get() {
            self.touch(index);
        }
        for index in 0..look.shown {
            let slot = index as usize;
            let (thickness, length, offset) = Self::targets(state, look.shown, index);
            self.thickness[slot].to(thickness);
            self.length[slot].to(length);
            self.offset[slot].to(offset);
            let (numbers, icon, margin, scale) = Self::marks(state, look, index);
            self.numbers[slot].to(numbers);
            self.icon_alpha[slot].to(icon);
            self.icon_margin[slot].to(margin);
            self.icon_scale[slot].to(scale);
        }
        self.aim_special(state);
    }
}

fn focus(workspace: &str) {
    hypr::request(&format!(
        "dispatch hl.dsp.focus({{workspace = {workspace}}})"
    ));
}

fn refresh(state: &Rc<RefCell<State>>, snapshot: &Snapshot, monitor: &str) {
    let mut state = state.borrow_mut();
    if let Some(monitors) = &snapshot.monitors {
        let mine = monitors
            .iter()
            .find(|entry| entry.get("name").and_then(Value::as_str) == Some(monitor));
        if let Some(active) = mine
            .and_then(|entry| entry.pointer("/activeWorkspace/id"))
            .and_then(Value::as_i64)
        {
            state.active = active as i32;
        }
        state.special = mine
            .and_then(|entry| entry.pointer("/specialWorkspace/name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim_start_matches("special:")
            .to_owned();
    }
    let fake = if snapshot.active_window.is_some() {
        None
    } else {
        Some(state.active)
    };
    if let Some(workspaces) = &snapshot.workspaces {
        state.occupied = workspaces
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_i64))
            .map(|id| id as i32)
            .filter(|id| Some(*id) != fake)
            .collect();
    }
    if let Some(clients) = &snapshot.clients {
        let mut biggest: HashMap<i32, (i64, String)> = HashMap::new();
        for client in clients {
            let Some(workspace) = client.pointer("/workspace/id").and_then(Value::as_i64) else {
                continue;
            };
            let area = client
                .get("size")
                .and_then(Value::as_array)
                .map(|size| {
                    let side = |at: usize| size.get(at).and_then(Value::as_i64).unwrap_or(0);
                    side(0) * side(1)
                })
                .unwrap_or(0);
            let class = client
                .get("class")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let held = biggest.get(&(workspace as i32)).map_or(0, |slot| slot.0);
            if area > held {
                biggest.insert(workspace as i32, (area, class.to_owned()));
            }
        }
        state.classes = biggest
            .into_iter()
            .map(|(workspace, (_, class))| (workspace, class))
            .collect();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    cr: &cairo::Context,
    area: &Paint,
    width: i32,
    height: i32,
    look: &Look,
    state: &State,
    motion: &Motion,
    theme: &crate::ui::theme::Theme,
) {
    let span = (BUTTON * look.shown as f64) as i32;
    let (width, height) = if look.vertical {
        (width, span)
    } else {
        (span, height)
    };
    let centre = if look.vertical {
        width as f64 / 2.0
    } else {
        height as f64 / 2.0
    };
    let blur = motion.special.get();

    if blur <= 0.002 {
        regular(cr, area, centre, look, state, motion, theme);
    } else if let Some((smeared, padding)) = smear(
        area, width, height, centre, look, state, motion, theme, blur,
    ) {
        let (centre_x, centre_y) = (width as f64 / 2.0, height as f64 / 2.0);
        let scale = 1.0 - 0.08 * blur;
        let _ = cr.save();
        cr.translate(centre_x, centre_y);
        cr.scale(scale, scale);
        cr.translate(-centre_x, -centre_y);
        let _ = cr.set_source_surface(&smeared, -padding, -padding);
        let _ = cr.paint();
        cr.set_operator(cairo::Operator::Atop);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.1 * blur);
        let _ = cr.paint();
        cr.set_operator(cairo::Operator::Over);
        let _ = cr.restore();
    }

    special_pill(cr, width, height, look, state, motion, theme, blur);
}

#[allow(clippy::too_many_arguments)]
fn smear(
    area: &Paint,
    width: i32,
    height: i32,
    centre: f64,
    look: &Look,
    state: &State,
    motion: &Motion,
    theme: &crate::ui::theme::Theme,
    blur: f64,
) -> Option<(cairo::ImageSurface, f64)> {
    let radius = (blur * BLUR_MAX / 5.0).round() as i32;
    let padding = radius * 3;
    let mut surface = cairo::ImageSurface::create(
        cairo::Format::ARgb32,
        width + 2 * padding,
        height + 2 * padding,
    )
    .ok()?;
    {
        let context = cairo::Context::new(&surface).ok()?;
        context.translate(padding as f64, padding as f64);
        regular(&context, area, centre, look, state, motion, theme);
    }
    box_blur(&mut surface, radius);
    Some((surface, padding as f64))
}

fn regular(
    cr: &cairo::Context,
    area: &Paint,
    centre: f64,
    look: &Look,
    state: &State,
    motion: &Motion,
    theme: &crate::ui::theme::Theme,
) {
    cr.push_group();
    set_source(cr, theme.m3.secondary_container);
    for index in 0..look.shown as usize {
        let thickness = motion.thickness[index].get();
        let length = motion.length[index].get();
        if thickness < 0.5 || length < 0.5 {
            continue;
        }
        capsule(
            cr,
            look.place(
                index as f64 * BUTTON + motion.offset[index].get(),
                centre - thickness / 2.0,
                length,
                thickness,
            ),
        );
        let _ = cr.fill();
    }
    let _ = cr.pop_group_to_source();
    let _ = cr.paint_with_alpha(0.6);

    let lead = motion.lead.get();
    let trail = motion.trail.get();
    let active = look.place(
        lead.min(trail) * BUTTON + ACTIVE_MARGIN,
        centre - ACTIVE_SIZE / 2.0,
        (lead - trail).abs() * BUTTON + ACTIVE_SIZE,
        ACTIVE_SIZE,
    );
    set_source(cr, theme.colors.col_primary);
    capsule(cr, active);
    let _ = cr.fill();

    let state_layer = DRAG_LAYER
        + if motion.hovered.get() {
            HOVER_LAYER
        } else {
            0.0
        }
        + if motion.pressed.get() {
            PRESS_LAYER
        } else {
            0.0
        };
    let touch_lead = motion.touch_lead.get();
    let touch_trail = motion.touch_trail.get();
    set_source(
        cr,
        transparentize(theme.colors.col_primary, 1.0 - state_layer as f32),
    );
    capsule(
        cr,
        look.place(
            touch_lead.min(touch_trail) * BUTTON + ACTIVE_MARGIN,
            centre - ACTIVE_SIZE / 2.0,
            (touch_lead - touch_trail).abs() * BUTTON + ACTIVE_SIZE,
            ACTIVE_SIZE,
        ),
    );
    let _ = cr.fill();

    marks(cr, centre, look, state, motion, theme, None);

    let _ = cr.save();
    capsule(cr, active);
    cr.clip();
    marks(
        cr,
        centre,
        look,
        state,
        motion,
        theme,
        Some(colorized(
            theme.colors.col_on_primary,
            theme.colors.col_primary,
            theme.colors.col_on_secondary_container,
        )),
    );
    let _ = cr.restore();

    icons(cr, area, centre, look, state, motion, theme);
}

fn colorized(tint: RGBA, source: RGBA, reference: RGBA) -> RGBA {
    let grey = 0.299 * source.red() + 0.587 * source.green() + 0.114 * source.blue();
    let high = reference.red().max(reference.green()).max(reference.blue());
    let low = reference.red().min(reference.green()).min(reference.blue());
    let brightness = 1.0 - (high + low) / 2.0;
    RGBA::new(
        (tint.red() * grey + brightness).min(1.0),
        (tint.green() * grey + brightness).min(1.0),
        (tint.blue() * grey + brightness).min(1.0),
        1.0,
    )
}

fn marks(
    cr: &cairo::Context,
    centre: f64,
    look: &Look,
    state: &State,
    motion: &Motion,
    theme: &crate::ui::theme::Theme,
    override_color: Option<RGBA>,
) {
    let extent = if look.vertical { centre * 2.0 } else { BAR };
    let group = (state.active - 1).div_euclid(look.shown);
    for index in 0..look.shown {
        let slot = index as usize;
        let id = group * look.shown + index + 1;
        let color = override_color.unwrap_or(if state.occupied.contains(&id) {
            theme.colors.col_on_secondary_container
        } else {
            theme.colors.col_on_layer1_inactive
        });
        let numbers = motion.numbers[slot].get();
        let left = index as f64 * BUTTON;

        if numbers < 0.999 {
            set_source(cr, transparentize(color, numbers as f32));
            let (x, y) = look.point(
                left + centred(BUTTON, DOT) + DOT / 2.0,
                centre + centred(extent, DOT) - extent / 2.0 + DOT / 2.0,
            );
            cr.arc(x, y, DOT / 2.0, 0.0, 2.0 * PI);
            let _ = cr.fill();
        }
        if numbers > 0.001 {
            set_source(cr, transparentize(color, 1.0 - numbers as f32));
            let text = look
                .number_map
                .get((id - 1).max(0) as usize)
                .filter(|label| !label.is_empty())
                .cloned()
                .unwrap_or_else(|| id.to_string());
            let size = NUMBER_SIZE
                - (text.chars().count() as f64 - 1.0) * if text == "10" { 0.0 } else { 2.0 };
            let layout = pangocairo::functions::create_layout(cr);
            layout.set_font_description(Some(&text::font(look.font, size, "wght=450")));
            layout.set_text(&text);
            let (text_width, text_height) = layout.pixel_size();
            let (text_along, text_across) = look.point(text_width as f64, text_height as f64);
            let (x, y) = look.point(
                left + centred(BUTTON, text_along),
                centre + centred(extent, text_across) - extent / 2.0,
            );
            cr.move_to(x, y);
            pangocairo::functions::show_layout(cr, &layout);
        }
    }
}

fn centred(extent: f64, size: f64) -> f64 {
    ((extent - size) / 2.0).round()
}

fn icons(
    cr: &cairo::Context,
    area: &Paint,
    centre: f64,
    look: &Look,
    state: &State,
    motion: &Motion,
    theme: &crate::ui::theme::Theme,
) {
    if !look.show_app_icons {
        return;
    }
    let group = (state.active - 1).div_euclid(look.shown);
    let tint = if theme.m3.darkmode {
        theme.colors.col_on_secondary_container
    } else {
        theme.colors.col_on_primary
    };
    let amount = if look.monochrome_icons { 0.8 } else { 0.5 };

    for index in 0..look.shown {
        let slot = index as usize;
        let alpha = motion.icon_alpha[slot].get();
        if alpha <= 0.002 {
            continue;
        }
        let id = group * look.shown + index + 1;
        let Some(class) = state.classes.get(&id) else {
            continue;
        };
        let Some(surface) = appicon::surface(area, class, ICON_SIZE, tint, amount) else {
            continue;
        };

        let margin = motion.icon_margin[slot].get();
        let (x, y) = look.point(
            (index + 1) as f64 * BUTTON - margin - ICON_SIZE as f64,
            centre + BUTTON / 2.0 - margin - ICON_SIZE as f64,
        );

        let _ = cr.save();
        let (centre_x, centre_icon_y) = (x + ICON_SIZE as f64 / 2.0, y + ICON_SIZE as f64 / 2.0);
        let scale = motion.icon_scale[slot].get();
        cr.translate(centre_x, centre_icon_y);
        cr.scale(scale, scale);
        cr.translate(-centre_x, -centre_icon_y);
        cr.arc(
            centre_x,
            centre_icon_y,
            ICON_SIZE as f64 / 2.0,
            0.0,
            2.0 * PI,
        );
        cr.clip();
        let _ = cr.set_source_surface(&surface, x, y);
        let _ = cr.paint_with_alpha(alpha);
        let _ = cr.restore();
    }
}

#[allow(clippy::too_many_arguments)]
fn special_pill(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    look: &Look,
    state: &State,
    motion: &Motion,
    theme: &crate::ui::theme::Theme,
    blur: f64,
) {
    if blur <= 0.002 {
        return;
    }
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_font_description(Some(&text::font(
        text::Family::Main,
        SPECIAL_TEXT_SIZE,
        "wght=450",
    )));
    layout.set_text(if look.vertical { "S" } else { &state.special });
    let (text_width, text_height) = layout.pixel_size();

    motion.special_length.to(if look.vertical {
        BUTTON * SPECIAL_VERTICAL_SHARE.min(look.shown as f64)
    } else {
        text_width as f64 + ACTIVE_SIZE
    });
    let length = motion.special_length.get().max(ACTIVE_SIZE);
    let scale = 0.8 + 0.2 * blur;
    let (centre_x, centre_y) = (width as f64 / 2.0, height as f64 / 2.0);
    let (centre_along, centre_across) = look.point(centre_x, centre_y);

    let _ = cr.save();
    cr.translate(centre_x, centre_y);
    cr.scale(scale, scale);
    cr.translate(-centre_x, -centre_y);

    cr.push_group();
    set_source(cr, theme.colors.col_primary);
    capsule(
        cr,
        look.place(
            centre_along - length / 2.0,
            centre_across - ACTIVE_SIZE / 2.0,
            length,
            ACTIVE_SIZE,
        ),
    );
    let _ = cr.fill();

    set_source(cr, theme.colors.col_on_primary);
    cr.move_to(
        centre_x - text_width as f64 / 2.0,
        centre_y - text_height as f64 / 2.0,
    );
    pangocairo::functions::show_layout(cr, &layout);
    let _ = cr.pop_group_to_source();
    let _ = cr.paint_with_alpha(blur);
    let _ = cr.restore();
}

fn box_blur(surface: &mut cairo::ImageSurface, radius: i32) {
    if radius < 1 {
        return;
    }
    let width = surface.width() as usize;
    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let mut scratch = vec![0u8; stride * height];
    {
        let Ok(mut data) = surface.data() else {
            return;
        };
        for _ in 0..3 {
            pass(&data, &mut scratch, width, height, stride, radius, true);
            pass(&scratch, &mut data, width, height, stride, radius, false);
        }
    }
    surface.mark_dirty();
}

#[allow(clippy::too_many_arguments)]
fn pass(
    source: &[u8],
    target: &mut [u8],
    width: usize,
    height: usize,
    stride: usize,
    radius: i32,
    horizontal: bool,
) {
    let window = (2 * radius + 1) as u32;
    for row in 0..height {
        for column in 0..width {
            let mut sums = [0u32; 4];
            for step in -radius..=radius {
                let (x, y) = if horizontal {
                    (column as i32 + step, row as i32)
                } else {
                    (column as i32, row as i32 + step)
                };
                if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                    continue;
                }
                let at = y as usize * stride + x as usize * 4;
                for (channel, sum) in sums.iter_mut().enumerate() {
                    *sum += source[at + channel] as u32;
                }
            }
            let at = row * stride + column * 4;
            for (channel, sum) in sums.iter().enumerate() {
                target[at + channel] = (sum / window) as u8;
            }
        }
    }
}

fn capsule(cr: &cairo::Context, (x, y, width, height): (f64, f64, f64, f64)) {
    let radius = width.min(height) / 2.0;
    cr.new_sub_path();
    if width >= height {
        cr.arc(x + width - radius, y + radius, radius, -PI / 2.0, PI / 2.0);
        cr.arc(x + radius, y + radius, radius, PI / 2.0, 3.0 * PI / 2.0);
    } else {
        cr.arc(x + radius, y + height - radius, radius, 0.0, PI);
        cr.arc(x + radius, y + radius, radius, PI, 2.0 * PI);
    }
    cr.close_path();
}

fn set_source(cr: &cairo::Context, color: RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blur_pass_fades_content_at_the_edge_into_transparency() {
        let mut source = vec![0u8; 5 * 4];
        source[..4].copy_from_slice(&[255; 4]);
        let mut target = vec![0u8; 5 * 4];
        pass(&source, &mut target, 5, 1, 5 * 4, 1, true);
        assert_eq!(&target[..8], &[85, 85, 85, 85, 85, 85, 85, 85]);
    }
}
