use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::config;
use crate::core::i18n::tr;
use crate::services::Services;
use crate::services::brightness::Change;
use crate::services::states;
use crate::ui::anim::{EMPHASIZED_DECEL, Tween};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::{self, Centred};
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::progress::{self, ProgressBar};
use crate::ui::widgets::text;

const NAMESPACE: &str = "proscenio:onScreenDisplay";
const OSD_WIDTH: i32 = 180;
const ELEVATION: i32 = 10;
const ICON_SLOT: i32 = 30;
const LEFT_PADDING: i32 = 10;
const RIGHT_PADDING: i32 = 20;
const VERTICAL_PADDING: i32 = 9;
const BAR_HEIGHT: i32 = 4;
const MOVE_MILLIS: f64 = 400.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Indicator {
    Volume,
    Brightness,
    Gamma,
}

impl Indicator {
    fn kind(self) -> states::Osd {
        match self {
            Indicator::Volume => states::Osd::Volume,
            _ => states::Osd::Brightness,
        }
    }
}

struct Reading {
    icon: &'static str,
    name: &'static str,
    value: f64,
    from: f64,
    rotate_icon: bool,
    scale_icon: bool,
}

impl Reading {
    fn icon_size(&self) -> f64 {
        20.0 + 10.0 * if self.scale_icon { self.value } else { 1.0 }
    }

    fn rotation(&self) -> f64 {
        180.0 * if self.rotate_icon { self.value } else { 0.0 }
    }

    fn position(&self) -> f64 {
        ((self.value - self.from) / (1.0 - self.from)).clamp(0.0, 1.0)
    }
}

struct Glyph {
    layout: RefCell<Option<pango::Layout>>,
    colour: gdk::RGBA,
    aligned: bool,
    size: Cell<Tween>,
    rotation: Cell<Tween>,
    shown: Cell<(f64, f64)>,
}

impl Glyph {
    fn draw(&self, snapshot: &gtk4::Snapshot) {
        let Some(layout) = self.layout.borrow().clone() else {
            return;
        };
        let (size, rotation) = self.shown.get();
        layout.set_font_description(Some(&text::symbol_font(size, 0.0, 400.0)));
        let Some((height, ascent)) = centred::layout_qt_metrics(&layout) else {
            return;
        };
        let width = layout.extents().1.width() as f32 / pango::SCALE as f32;
        let baseline = layout.baseline() as f32 / pango::SCALE as f32;
        let slot = ICON_SLOT as f32;
        let (mut x, mut y) = ((slot - width) / 2.0, (slot - height) / 2.0);
        if self.aligned {
            (x, y) = (x.round(), y.round());
        }
        let centre = slot / 2.0;
        snapshot.save();
        snapshot.translate(&graphene::Point::new(centre, centre));
        snapshot.rotate(rotation as f32);
        snapshot.translate(&graphene::Point::new(
            x - centre,
            y + ascent - baseline - centre,
        ));
        snapshot.append_layout(&layout, &self.colour);
        snapshot.restore();
    }
}

struct Value {
    indicator: Indicator,
    root: gtk4::Box,
    icon: Paint,
    glyph: Rc<Glyph>,
    name: gtk4::Label,
    reading: gtk4::Label,
    bar: ProgressBar,
    ticking: Cell<bool>,
}

impl Value {
    fn new(indicator: Indicator, reading: &Reading, theme: &SharedTheme) -> Rc<Self> {
        let glyph = Rc::new(Glyph {
            layout: RefCell::new(None),
            colour: theme.borrow().colors.col_on_layer0,
            aligned: !reading.rotate_icon,
            size: Cell::new(Tween::new(
                reading.icon_size(),
                MOVE_MILLIS,
                EMPHASIZED_DECEL,
            )),
            rotation: Cell::new(Tween::new(
                reading.rotation(),
                MOVE_MILLIS,
                EMPHASIZED_DECEL,
            )),
            shown: Cell::new((reading.icon_size(), reading.rotation())),
        });
        let icon = Paint::new({
            let glyph = glyph.clone();
            move |snapshot, _, _| glyph.draw(snapshot)
        });
        glyph
            .layout
            .replace(Some(icon.create_pango_layout(Some(reading.icon))));
        icon.set_size_request(ICON_SLOT, ICON_SLOT);
        icon.set_valign(gtk4::Align::Center);
        icon.set_margin_start(LEFT_PADDING);
        icon.set_margin_top(VERTICAL_PADDING);
        icon.set_margin_bottom(VERTICAL_PADDING);

        let name = text::styled(&tr(reading.name));
        text::set_color(&name, "colOnLayer0");
        name.set_xalign(0.0);
        let name_holder = Centred::filling_width(&name);
        name_holder.set_hexpand(true);
        let value = text::styled(&percent(reading.value));
        text::set_color(&value, "colOnLayer0");

        let caption = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        caption.set_margin_start(BAR_HEIGHT / 2);
        caption.set_margin_end(BAR_HEIGHT / 2);
        caption.append(&name_holder);
        caption.append(&Centred::new(&value));

        let bar = ProgressBar::new();
        bar.set_natural_width(0);
        bar.set_hexpand(true);
        {
            let theme = theme.borrow();
            bar.set_colours(progress::Colours {
                highlight: theme.colors.col_primary,
                track: theme.m3.secondary_container,
            });
        }
        bar.set_value(reading.position());

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        column.set_valign(gtk4::Align::Center);
        column.set_hexpand(true);
        column.set_margin_end(RIGHT_PADDING);
        column.append(&caption);
        column.append(&bar);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row.add_css_class("osd");
        row.set_size_request(OSD_WIDTH, -1);
        row.append(&icon);
        row.append(&column);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_halign(gtk4::Align::Start);
        root.set_margin_top(ELEVATION);
        root.set_margin_bottom(ELEVATION);
        root.set_margin_start(ELEVATION);
        root.set_margin_end(ELEVATION);
        root.append(&row);

        Rc::new(Value {
            indicator,
            root,
            icon,
            glyph,
            name,
            reading: value,
            bar,
            ticking: Cell::new(false),
        })
    }

    fn update(self: &Rc<Self>, reading: &Reading) {
        if let Some(layout) = self.glyph.layout.borrow().as_ref() {
            layout.set_text(reading.icon);
        }
        self.icon.queue_draw();
        self.name.set_text(&tr(reading.name));
        self.reading.set_text(&percent(reading.value));
        self.bar.set_value(reading.position());

        let now = self
            .icon
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        for (tween, target) in [
            (&self.glyph.size, reading.icon_size()),
            (&self.glyph.rotation, reading.rotation()),
        ] {
            let mut next = tween.get();
            next.retarget(target, now);
            tween.set(next);
        }
        if self.ticking.replace(true) {
            return;
        }
        let value = self.clone();
        self.icon.add_tick_callback(move |icon, clock| {
            let now = clock.frame_time();
            let (size, rotation) = (value.glyph.size.get(), value.glyph.rotation.get());
            value
                .glyph
                .shown
                .set((size.value(now), rotation.value(now)));
            icon.queue_draw();
            if size.running(now) || rotation.running(now) {
                return glib::ControlFlow::Continue;
            }
            value.ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

struct Message {
    root: gtk4::Box,
    label: gtk4::Label,
}

impl Message {
    fn new() -> Self {
        let icon = text::symbol("dangerous", pixel_size::HUGEASS as f64);
        text::set_color(&icon, "m3onError");
        let icon = Centred::new(&icon);
        icon.set_valign(gtk4::Align::Center);

        let label = text::styled("");
        text::set_color(&label, "m3onError");
        label.set_justify(gtk4::Justification::Center);
        label.set_wrap(true);
        let holder = Centred::new(&label);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        row.add_css_class("osd-protection");
        row.append(&icon);
        row.append(&holder);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.set_halign(gtk4::Align::Center);
        root.append(&row);
        Message { root, label }
    }

    fn set(&self, message: &str) {
        self.label.set_text(message);
        self.root.set_visible(!message.is_empty());
    }
}

struct Shown {
    window: gtk4::ApplicationWindow,
    connector: String,
    content: gtk4::Box,
    value: Rc<Value>,
    message: Message,
}

pub struct Osd {
    app: gtk4::Application,
    services: Rc<Services>,
    theme: SharedTheme,
    current: Cell<Indicator>,
    message: RefCell<String>,
    timeout: RefCell<Option<glib::SourceId>>,
    shown: RefCell<Option<Shown>>,
}

impl Osd {
    pub fn new(app: &gtk4::Application, services: &Rc<Services>, theme: &SharedTheme) -> Rc<Self> {
        let osd = Rc::new(Osd {
            app: app.clone(),
            services: services.clone(),
            theme: theme.clone(),
            current: Cell::new(Indicator::Volume),
            message: RefCell::new(String::new()),
            timeout: RefCell::new(None),
            shown: RefCell::new(None),
        });

        services
            .light
            .subscribe({
                let osd = osd.clone();
                move |change| {
                    let indicator = match change {
                        Change::Read => return,
                        Change::Level => Indicator::Brightness,
                        Change::Gamma => Indicator::Gamma,
                    };
                    osd.message.borrow_mut().clear();
                    osd.current.set(indicator);
                    osd.trigger();
                }
            })
            .forever();
        if let Some(audio) = &services.audio {
            audio
                .on_sink_change({
                    let osd = osd.clone();
                    move || {
                        osd.current.set(Indicator::Volume);
                        osd.trigger();
                    }
                })
                .forever();
            audio
                .on_protection({
                    let osd = osd.clone();
                    move |reason| {
                        osd.message.replace(reason.to_owned());
                        osd.current.set(Indicator::Volume);
                        osd.trigger();
                    }
                })
                .forever();
        }
        services
            .states
            .on_osd_close({
                let osd = osd.clone();
                move |kind| {
                    if osd.current.get().kind() == kind {
                        osd.hide();
                    }
                }
            })
            .forever();
        services
            .events
            .subscribe({
                let osd = osd.clone();
                move |event, _| {
                    if !matches!(event, "focusedmon" | "focusedmonv2") {
                        return;
                    }
                    let moved = osd.shown.borrow().as_ref().is_some_and(|shown| {
                        crate::platform::hypr::focused_monitor()
                            .is_some_and(|name| name != shown.connector)
                    });
                    if moved {
                        osd.hide();
                        osd.present();
                    }
                }
            })
            .forever();
        osd
    }

    pub fn trigger(self: &Rc<Self>) {
        self.present();
        if let Some(pending) = self.timeout.borrow_mut().take() {
            pending.remove();
        }
        let osd = self.clone();
        let source = glib::timeout_add_local_once(
            Duration::from_millis(
                config::value_i64("/osd/timeout", config::OSD_TIMEOUT).max(1) as u64,
            ),
            move || {
                osd.timeout.borrow_mut().take();
                osd.hide();
                osd.message.borrow_mut().clear();
            },
        );
        self.timeout.replace(Some(source));
    }

    pub fn hide(&self) {
        if let Some(shown) = self.shown.borrow_mut().take() {
            shown.window.destroy();
        }
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.shown.borrow().is_some() {
            self.hide();
            return;
        }
        if let Some(pending) = self.timeout.borrow_mut().take() {
            pending.remove();
        }
        self.present();
    }

    fn reading(&self, indicator: Indicator) -> Reading {
        match indicator {
            Indicator::Volume => {
                let (volume, muted) = self.services.audio.as_ref().map_or((0.0, false), |audio| {
                    (audio.sink_volume.get(), audio.sink_muted.get())
                });
                Reading {
                    icon: if muted { "volume_off" } else { "volume_up" },
                    name: "Volume",
                    value: volume,
                    from: 0.0,
                    rotate_icon: false,
                    scale_icon: false,
                }
            }
            Indicator::Brightness => Reading {
                icon: if self.services.session.night.get() {
                    "routine"
                } else {
                    "light_mode"
                },
                name: "Brightness",
                value: self
                    .services
                    .light
                    .level(&crate::platform::hypr::focused_monitor().unwrap_or_default()),
                from: 0.0,
                rotate_icon: true,
                scale_icon: true,
            },
            Indicator::Gamma => Reading {
                icon: "wb_twilight",
                name: "Gamma",
                value: self.services.light.gamma.get() / 100.0,
                from: crate::services::brightness::GAMMA_FLOOR / 100.0,
                rotate_icon: false,
                scale_icon: false,
            },
        }
    }

    fn present(self: &Rc<Self>) {
        let indicator = self.current.get();
        let reading = self.reading(indicator);
        if self.shown.borrow().is_none() {
            let Some(monitor) = focused_monitor() else {
                return;
            };
            let shown = self.open(&monitor, indicator, &reading);
            self.shown.replace(Some(shown));
        }
        let mut held = self.shown.borrow_mut();
        let Some(shown) = held.as_mut() else {
            return;
        };
        if shown.value.indicator == indicator {
            shown.value.update(&reading);
        } else {
            let value = Value::new(indicator, &reading, &self.theme);
            shown.content.remove(&shown.value.root);
            shown.content.prepend(&value.root);
            shown.value = value;
        }
        shown.message.set(&self.message.borrow());
    }

    fn open(
        self: &Rc<Self>,
        monitor: &gdk::Monitor,
        indicator: Indicator,
        reading: &Reading,
    ) -> Shown {
        let value = Value::new(indicator, reading, &self.theme);
        let message = Message::new();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&value.root);
        content.append(&message.root);

        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .child(&content)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        window.set_monitor(Some(monitor));
        window.set_layer(Layer::Overlay);
        let config = config::current();
        let edge = if config.bottom {
            Edge::Bottom
        } else {
            Edge::Top
        };
        window.set_anchor(edge, true);
        window.set_margin(edge, config.bar_height());
        window.set_exclusive_zone(-1);

        let hover = gtk4::EventControllerMotion::new();
        hover.connect_enter({
            let osd = Rc::downgrade(self);
            move |_, _, _| {
                if let Some(osd) = osd.upgrade() {
                    osd.hide();
                }
            }
        });
        content.add_controller(hover);
        window.present();

        Shown {
            window,
            connector: monitor.connector().map(Into::into).unwrap_or_default(),
            content,
            value,
            message,
        }
    }
}

fn focused_monitor() -> Option<gdk::Monitor> {
    let wanted = crate::platform::hypr::focused_monitor();
    let monitors = gdk::Display::default()?.monitors();
    let all: Vec<gdk::Monitor> = monitors.iter::<gdk::Monitor>().flatten().collect();
    all.iter()
        .find(|monitor| monitor.connector().map(|name| name.to_string()) == wanted)
        .or(all.first())
        .cloned()
}

fn percent(value: f64) -> String {
    format!("{}", (value * 100.0).round() as i64)
}
