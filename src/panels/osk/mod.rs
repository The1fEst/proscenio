pub mod key;
pub mod layouts;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config;
use crate::core::scope::Scope;
use crate::core::watch;
use crate::platform::system::ydotool::Ydotool;
use crate::platform::wayland::grab;
use crate::services::session::states::States;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::unload;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::group::{ButtonGroup, GroupButton};
use key::OskKey;

const NAMESPACE: &str = "proscenio:osk";
const PINNED_ON_STARTUP: &str = "/osk/pinnedOnStartup";
const LAYOUT: &str = "/osk/layout";
const ELEVATION: i32 = 10;
const GAPS_OUT: i32 = 5;
const PADDING: i32 = 10;
const SPACING: i32 = 5;
const CONTROL: f64 = 40.0;
const CONTROL_PRESSED: f64 = 50.0;
const SEPARATOR_MARGIN: i32 = 20;

pub struct Osk {
    app: gtk4::Application,
    theme: SharedTheme,
    states: States,
    ydotool: Ydotool,
    is_pinned: Rc<Cell<bool>>,
    is_pin_touched: Cell<bool>,
    open_window: RefCell<Option<Shown>>,
    owned_items: RefCell<Vec<Box<dyn std::any::Any>>>,
}

struct Shown {
    window: gtk4::ApplicationWindow,
    frame: gtk4::Box,
    pin_button: GroupButton,
    pin_icon: gtk4::Label,
    _scope: Scope,
}

impl Osk {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, states: &States) -> Rc<Self> {
        let osk = Rc::new(Osk {
            app: app.clone(),
            theme: theme.clone(),
            states: states.clone(),
            ydotool: Ydotool::default(),
            is_pinned: Rc::new(Cell::new(config::value_bool(PINNED_ON_STARTUP, false))),
            is_pin_touched: Cell::new(false),
            open_window: RefCell::new(None),
            owned_items: RefCell::new(Vec::new()),
        });
        let subscription = states.subscribe({
            let osk = Rc::downgrade(&osk);
            move || {
                if let Some(osk) = osk.upgrade() {
                    osk.sync();
                }
            }
        });
        let startup_watch = watch::config(PINNED_ON_STARTUP, {
            let osk = Rc::downgrade(&osk);
            move || {
                let Some(osk) = osk.upgrade() else {
                    return;
                };
                if !osk.is_pin_touched.get() {
                    osk.set_pinned(config::value_bool(PINNED_ON_STARTUP, false));
                }
            }
        });
        let layout_watch = watch::config(LAYOUT, {
            let osk = Rc::downgrade(&osk);
            move || {
                if let Some(osk) = osk.upgrade()
                    && osk.open_window.borrow().is_some()
                {
                    osk.close_window();
                    osk.sync();
                }
            }
        });
        osk.owned_items.replace(vec![
            Box::new(subscription),
            Box::new(startup_watch),
            Box::new(layout_watch),
        ]);
        osk
    }

    pub fn toggle(&self) {
        self.states.set_osk_open(!self.states.is_osk_open.get());
    }

    pub fn open(&self) {
        self.states.set_osk_open(true);
    }

    pub fn close(&self) {
        self.states.set_osk_open(false);
    }

    fn sync(self: &Rc<Self>) {
        let should_be_open = self.states.is_osk_open.get();
        let is_open = self.open_window.borrow().is_some();
        if should_be_open && !is_open {
            self.open_window.replace(Some(self.build()));
        } else if !should_be_open && is_open {
            self.close_window();
        }
        if let Some(open_window) = self.open_window.borrow().as_ref() {
            open_window
                .window
                .set_visible(!self.states.is_screen_locked.get());
        }
    }

    fn close_window(&self) {
        let Some(open_window) = self.open_window.take() else {
            return;
        };
        if let Some(surface) = open_window.window.surface() {
            grab::remove_persistent(&surface);
        }
        unload::discard(open_window.window.upcast_ref());
        self.ydotool.release_all_keys();
    }

    fn set_pinned(&self, is_pinned: bool) {
        self.is_pinned.set(is_pinned);
        if let Some(open_window) = self.open_window.borrow().as_ref() {
            open_window.show_pin(is_pinned);
        }
    }

    fn build(self: &Rc<Self>) -> Shown {
        let scope = Scope::default();
        let layout = layouts::named(&config::value_str(LAYOUT).unwrap_or_default());
        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);
        rows.set_hexpand(true);
        for keys in layout.rows {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
            for key in keys.iter() {
                let osk_key = OskKey::new(&self.theme, key, &self.ydotool, &scope);
                osk_key.button.set_valign(gtk4::Align::Center);
                row.append(&osk_key.button);
                scope.hold(osk_key);
            }
            rows.append(&row);
        }

        let (pin_button, pin_icon) = control(&self.theme, "keep");
        pin_button.connect_down({
            let osk = Rc::downgrade(self);
            move || {
                if let Some(osk) = osk.upgrade() {
                    osk.is_pin_touched.set(true);
                    osk.set_pinned(!osk.is_pinned.get());
                }
            }
        });
        let (hide_button, _) = control(&self.theme, "keyboard_hide");
        hide_button.connect_clicked({
            let osk = Rc::downgrade(self);
            move || {
                if let Some(osk) = osk.upgrade() {
                    osk.close();
                }
            }
        });
        let controls = ButtonGroup::new(&self.theme);
        controls.set_vertical(true);
        controls.set_valign(gtk4::Align::Center);
        controls.append(&pin_button);
        controls.append(&hide_button);

        let separator = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        separator.add_css_class("osk-separator");
        separator.set_size_request(1, -1);
        separator.set_margin_top(SEPARATOR_MARGIN);
        separator.set_margin_bottom(SEPARATOR_MARGIN);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        row.set_margin_top(PADDING);
        row.set_margin_bottom(PADDING);
        row.set_margin_start(PADDING);
        row.set_margin_end(PADDING);
        row.append(&controls);
        row.append(&separator);
        row.append(&rows);

        let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        frame.add_css_class("osk");
        frame.set_halign(gtk4::Align::Center);
        frame.set_valign(gtk4::Align::Center);
        frame.set_margin_top(ELEVATION);
        frame.set_margin_bottom(ELEVATION);
        frame.set_margin_start(ELEVATION);
        frame.set_margin_end(ELEVATION);
        frame.append(&row);

        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .child(&frame)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        window.set_layer(Layer::Overlay);
        window.set_anchor(Edge::Bottom, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_keyboard_mode(KeyboardMode::None);
        window.connect_realize(|window| {
            if let Some(surface) = window.surface() {
                grab::add_persistent(&surface);
            }
        });
        window.present();
        frame.add_tick_callback(|frame, _| {
            let Some(window) = frame.root().and_downcast::<gtk4::Window>() else {
                return glib::ControlFlow::Break;
            };
            let (Some(surface), true) = (window.surface(), window.width() > 0) else {
                return glib::ControlFlow::Continue;
            };
            let width = frame.width();
            let region = gtk4::cairo::RectangleInt::new(
                (window.width() - width) / 2,
                ELEVATION,
                width,
                frame.height(),
            );
            surface.set_input_region(Some(&gtk4::cairo::Region::create_rectangle(&region)));
            glib::ControlFlow::Break
        });

        let open_window = Shown {
            window,
            frame,
            pin_button,
            pin_icon,
            _scope: scope,
        };
        open_window.show_pin(self.is_pinned.get());
        open_window
    }
}

impl Shown {
    fn show_pin(&self, is_pinned: bool) {
        self.pin_button.set_toggled(is_pinned);
        text::set_color(
            &self.pin_icon,
            if is_pinned {
                "m3onPrimary"
            } else {
                "colOnLayer0"
            },
        );
        let height = self.frame.measure(gtk4::Orientation::Vertical, -1).1;
        self.window
            .set_exclusive_zone(if is_pinned { height - GAPS_OUT } else { 0 });
    }
}

fn control(theme: &SharedTheme, icon: &str) -> (GroupButton, gtk4::Label) {
    let button = GroupButton::new(theme, CONTROL, CONTROL);
    button.set_clicked_width(CONTROL);
    button.set_clicked_height(CONTROL_PRESSED);
    button.set_radii(rounding::NORMAL as f64, rounding::SMALL as f64);
    let symbol = text::symbol(icon, pixel_size::LARGER as f64);
    button.set_content(&Centered::new(&symbol));
    (button, symbol)
}
