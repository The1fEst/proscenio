pub mod arrangement;
pub mod content;
pub mod hyprrows;
pub mod pages;
mod rail;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::{config, watch};
use crate::platform::hypr;
use crate::services::Services;
use crate::ui::anim::{EMPHASIZED, Ease};
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::text::{self, Family};
use content::{Context, Page};
use pages::PAGES;
use rail::Rail;

const TITLE: &str = "proscenio Settings";
const WIDTH: i32 = 1100;
const HEIGHT: i32 = 750;
const MIN_WIDTH: i32 = 750;
const MIN_HEIGHT: i32 = 500;
const PADDING: i32 = 8;
const COLUMN_SPACING: i32 = 5;
const TITLE_START: i32 = 12;
const CLOSE_SIZE: i32 = 35;
const CLOSE_ICON: f64 = 20.0;
const RAIL_MARGIN: i32 = 5;
const HEADER_MARGIN: i32 = 10;
const HEADER_SPACING: i32 = 8;
const SWITCH_OUT: f64 = 100_000.0;
const SWITCH_IN: f64 = 200_000.0;
const SWITCH_DROP: f64 = 20.0;
const FOCUS_POLL: std::time::Duration = std::time::Duration::from_millis(20);
const FOCUS_TRIES: u32 = 25;
const EMPHASIZED_LAST_HALF: Ease = Ease::Bezier(5.0 / 24.0, 0.82, 0.25, 1.0);

pub struct Settings {
    app: gtk4::Application,
    context: Rc<Context>,
    view: RefCell<Option<Rc<View>>>,
}

#[derive(Clone, Copy)]
struct Shown {
    id: &'static str,
    subpage: Option<&'static str>,
}

struct View {
    window: gtk4::ApplicationWindow,
    context: Rc<Context>,
    rail: Rc<Rail>,
    stage: gtk4::Box,
    header: gtk4::Box,
    header_title: gtk4::Label,
    page: RefCell<Option<Rc<Page>>>,
    loaded: Cell<Option<&'static str>>,
    wanted: Cell<Shown>,
    switch_start: Cell<i64>,
    switching: Cell<bool>,
    _following: watch::Watch,
}

impl Settings {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, services: &Rc<Services>) -> Rc<Self> {
        Rc::new(Settings {
            app: app.clone(),
            context: Rc::new(Context {
                theme: theme.clone(),
                services: services.clone(),
                subpage: RefCell::new(None),
            }),
            view: RefCell::new(None),
        })
    }

    pub fn open(self: &Rc<Self>, page: Option<&str>) {
        let index = page.and_then(pages::index_of);
        let existing = self.view.borrow().clone();
        let view = match existing {
            Some(view) => {
                focus_on_its_workspace();
                view
            }
            None => {
                let view = self.build();
                self.view.replace(Some(view.clone()));
                focus_once_mapped();
                view
            }
        };
        if let Some(index) = index {
            view.select(index);
        }
        if let Some(subpage) = page.and_then(pages::subpage) {
            if let Some(parent) = pages::index_of(subpage.parent) {
                view.select(parent);
            }
            view.show(Shown {
                id: subpage.id,
                subpage: Some(subpage.title),
            });
        }
        view.window.present();
    }

    pub fn close(&self) {
        if let Some(view) = self.view.take() {
            view.window.destroy();
        }
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.view.borrow().is_some() {
            self.close();
        } else {
            self.open(None);
        }
    }

    fn build(self: &Rc<Self>) -> Rc<View> {
        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .title(TITLE)
            .default_width(WIDTH)
            .default_height(HEIGHT)
            .decorated(false)
            .build();
        window.set_size_request(MIN_WIDTH, MIN_HEIGHT);
        window.add_css_class("settings-window");

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, COLUMN_SPACING);
        column.set_margin_top(PADDING);
        column.set_margin_bottom(PADDING);
        column.set_margin_start(PADDING);
        column.set_margin_end(PADDING);
        let (titlebar, title) = self.titlebar();
        column.append(&titlebar);
        let arrange = move || {
            titlebar.set_visible(config::value_bool("/windows/showTitlebar", true));
            titlebar.set_start_widget(gtk4::Widget::NONE);
            titlebar.set_center_widget(gtk4::Widget::NONE);
            if config::value_bool("/windows/centerTitle", true) {
                title.set_margin_start(0);
                titlebar.set_center_widget(Some(&title));
            } else {
                title.set_margin_start(TITLE_START);
                titlebar.set_start_widget(Some(&title));
            }
        };
        arrange();
        let following = watch::config("/windows", arrange);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, PADDING);
        row.set_vexpand(true);
        let rail = Rail::new(&self.context.theme);
        rail.root.set_margin_top(RAIL_MARGIN);
        rail.root.set_margin_bottom(RAIL_MARGIN);
        rail.root.set_margin_start(RAIL_MARGIN);
        rail.root.set_margin_end(RAIL_MARGIN);
        row.append(&rail.root);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.add_css_class("settings-content");
        content.set_hexpand(true);
        content.set_vexpand(true);
        let (header, header_title, back) = self.subpage_header();
        content.append(&header);
        let stage = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        stage.set_vexpand(true);
        content.append(&stage);
        row.append(&content);
        column.append(&row);
        window.set_child(Some(&column));

        let view = Rc::new(View {
            window: window.clone(),
            context: self.context.clone(),
            rail: rail.clone(),
            stage,
            header,
            header_title,
            page: RefCell::new(None),
            loaded: Cell::new(None),
            wanted: Cell::new(Shown {
                id: PAGES[0].id,
                subpage: None,
            }),
            switch_start: Cell::new(0),
            switching: Cell::new(false),
            _following: following,
        });
        self.context.subpage.replace(Some(Rc::new({
            let view = Rc::downgrade(&view);
            move |name, id| {
                if let Some(view) = view.upgrade() {
                    view.show(Shown {
                        id,
                        subpage: Some(name),
                    });
                }
            }
        })));
        view.load();
        back.connect_clicked({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.select(view.rail.current());
                }
            }
        });

        rail.connect_selected({
            let view = Rc::downgrade(&view);
            move |page| {
                if let Some(view) = view.upgrade() {
                    view.select(page);
                }
            }
        });
        rail.follow_window_width(WIDTH);
        window.connect_realize({
            let rail = Rc::downgrade(&rail);
            move |window| {
                let Some(surface) = window.surface() else {
                    return;
                };
                let rail = rail.clone();
                surface.connect_layout(move |_, width, _| {
                    if let Some(rail) = rail.upgrade() {
                        rail.follow_window_width(width);
                    }
                });
            }
        });

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let settings = Rc::downgrade(self);
            let view = Rc::downgrade(&view);
            move |_, key, _, modifiers| {
                let (Some(settings), Some(view)) = (settings.upgrade(), view.upgrade()) else {
                    return glib::Propagation::Proceed;
                };
                if key == gdk::Key::Escape {
                    settings.close();
                    return glib::Propagation::Stop;
                }
                if modifiers != gdk::ModifierType::CONTROL_MASK {
                    return glib::Propagation::Proceed;
                }
                let current = view.rail.current();
                let last = PAGES.len() - 1;
                let next = match key {
                    gdk::Key::Page_Down => (current + 1).min(last),
                    gdk::Key::Page_Up => current.saturating_sub(1),
                    gdk::Key::Tab => (current + 1) % PAGES.len(),
                    gdk::Key::ISO_Left_Tab => (current + last) % PAGES.len(),
                    _ => return glib::Propagation::Proceed,
                };
                view.select(next);
                glib::Propagation::Stop
            }
        });
        window.add_controller(keys);

        window.connect_close_request({
            let settings = Rc::downgrade(self);
            move |_| {
                if let Some(settings) = settings.upgrade() {
                    settings.view.replace(None);
                }
                glib::Propagation::Proceed
            }
        });
        view
    }

    fn titlebar(self: &Rc<Self>) -> (gtk4::CenterBox, Centred) {
        let bar = gtk4::CenterBox::new();
        let title = gtk4::Label::new(Some("Settings"));
        text::set_font(&title, Family::Title, pixel_size::TITLE as f64, "wght=550");
        text::set_color(&title, "colOnLayer0");
        let placed = Centred::new(&title);
        placed.set_vexpand(true);

        let close = RippleButton::new(&self.context.theme);
        close.set_radius(rounding::FULL as f64);
        close.set_size_request(CLOSE_SIZE, CLOSE_SIZE);
        close.set_valign(gtk4::Align::Center);
        let icon = text::symbol("close", CLOSE_ICON);
        close.set_content(&Centred::integral(&icon), 0, 0);
        close.connect_clicked({
            let settings = Rc::downgrade(self);
            move |_| {
                if let Some(settings) = settings.upgrade() {
                    settings.close();
                }
            }
        });
        bar.set_end_widget(Some(&close));
        (bar, placed)
    }

    fn subpage_header(&self) -> (gtk4::Box, gtk4::Label, RippleButton) {
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, HEADER_SPACING);
        header.set_margin_top(HEADER_MARGIN);
        header.set_margin_bottom(HEADER_MARGIN);
        header.set_margin_start(HEADER_MARGIN);
        header.set_margin_end(HEADER_MARGIN);
        header.set_visible(false);
        let back = RippleButton::new(&self.context.theme);
        back.set_radius(rounding::FULL as f64);
        back.set_size_request(CLOSE_SIZE, CLOSE_SIZE);
        let icon = text::symbol("arrow_back", CLOSE_ICON);
        back.set_content(&Centred::integral(&icon), 0, 0);
        header.append(&back);
        let title = text::styled("");
        text::set_font(&title, Family::Main, pixel_size::LARGER as f64, "wght=450");
        text::set_color(&title, "colOnLayer1");
        title.set_xalign(0.0);
        let placed = Centred::filling_width(&title);
        placed.set_hexpand(true);
        header.append(&placed);
        (header, title, back)
    }
}

fn focus_on_its_workspace() -> bool {
    let own = std::process::id() as u64;
    let address = hypr::json("clients").and_then(|clients| {
        clients.as_array()?.iter().find_map(|client| {
            let ours = client.get("pid").and_then(Value::as_u64) == Some(own)
                && client.get("title").and_then(Value::as_str) == Some(TITLE);
            ours.then(|| client.get("address")?.as_str().map(str::to_owned))?
        })
    });
    let Some(address) = address else {
        return false;
    };
    hypr::focus_window(&address);
    true
}

fn focus_once_mapped() {
    let tries = Cell::new(0);
    glib::timeout_add_local(FOCUS_POLL, move || {
        tries.set(tries.get() + 1);
        if focus_on_its_workspace() || tries.get() >= FOCUS_TRIES {
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
}

impl View {
    fn select(self: &Rc<Self>, page: usize) {
        self.rail.set_current(page);
        self.show(Shown {
            id: PAGES[page].id,
            subpage: None,
        });
    }

    fn show(self: &Rc<Self>, shown: Shown) {
        self.header.set_visible(shown.subpage.is_some());
        self.header_title.set_text(shown.subpage.unwrap_or(""));
        let previous = self.wanted.replace(shown);
        if previous.id == shown.id && self.loaded.get() == Some(shown.id) {
            return;
        }
        self.switch_start.set(0);
        if self.switching.replace(true) {
            return;
        }
        let view = self.clone();
        self.stage.add_tick_callback(move |stage, clock| {
            let now = clock.frame_time();
            if view.switch_start.get() == 0 {
                view.switch_start.set(now);
            }
            let elapsed = (now - view.switch_start.get()) as f64;
            if elapsed < SWITCH_OUT {
                stage.set_margin_top(0);
                stage.set_opacity(1.0 - EMPHASIZED.at(elapsed / SWITCH_OUT));
                return glib::ControlFlow::Continue;
            }
            if view.loaded.get() != Some(view.wanted.get().id) {
                view.load();
            }
            let part = ((elapsed - SWITCH_OUT) / SWITCH_IN).min(1.0);
            let eased = EMPHASIZED_LAST_HALF.at(part);
            stage.set_opacity(eased);
            stage.set_margin_top((SWITCH_DROP * (1.0 - eased)).round() as i32);
            if part < 1.0 {
                return glib::ControlFlow::Continue;
            }
            view.switching.set(false);
            glib::ControlFlow::Break
        });
    }

    fn load(&self) {
        if let Some(root) = self.stage.root() {
            let inside = root
                .focus()
                .is_some_and(|focus| focus.is_ancestor(&self.stage));
            if inside {
                root.set_focus(None::<&gtk4::Widget>);
            }
        }
        if let Some(previous) = self.page.take() {
            self.stage.remove(&previous.root);
        }
        let wanted = self.wanted.get();
        let page = pages::build(wanted.id, wanted.subpage, &self.context);
        self.stage.append(&page.root);
        self.page.replace(Some(page));
        self.loaded.set(Some(wanted.id));
    }
}
