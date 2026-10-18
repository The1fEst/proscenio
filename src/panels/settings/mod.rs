pub mod arrangement;
pub mod content;
pub mod gestures;
pub mod hyprrows;
mod index;
pub mod pages;
mod rail;
#[cfg(test)]
mod scan;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::{tr, trn};
use crate::core::{config, watch};
use crate::platform::hyprland::hypr;
use crate::services::Services;
use crate::ui::anim::{EMPHASIZED, Ease};
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};
use content::{Context, HIGHLIGHT_CHANGED, Page};
use index::Hit;
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
const TOOL_SPACING: i32 = 4;
const RESET_WIDTH: f64 = 420.0;
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

impl Shown {
    fn of(id: &'static str) -> Self {
        Shown {
            id,
            subpage: pages::subpage(id).map(|subpage| subpage.title),
        }
    }
}

struct View {
    window: gtk4::ApplicationWindow,
    context: Rc<Context>,
    rail: Rc<Rail>,
    stage: gtk4::Box,
    header: gtk4::Box,
    header_title: gtk4::Label,
    page: RefCell<Option<Rc<Page>>>,
    loaded_id: Cell<Option<&'static str>>,
    target: Cell<Shown>,
    history: RefCell<Vec<Shown>>,
    pending_hit: RefCell<Option<Hit>>,
    switch_start: Cell<i64>,
    is_switching: Cell<bool>,
    tips: RefCell<Vec<Rc<Tooltip>>>,
    highlight_watch: RefCell<Option<watch::Watch>>,
    _layout_watch: watch::Watch,
}

impl Settings {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, services: &Rc<Services>) -> Rc<Self> {
        Rc::new(Settings {
            app: app.clone(),
            context: Rc::new(Context {
                theme: theme.clone(),
                services: services.clone(),
                on_open_subpage: RefCell::new(None),
                overlay: glib::WeakRef::new(),
                dialog: Rc::default(),
                argument: Rc::default(),
                heading: glib::WeakRef::new(),
                on_back: RefCell::new(None),
                shared: RefCell::new(None),
            }),
            view: RefCell::new(None),
        })
    }

    pub fn open(self: &Rc<Self>, page: Option<&str>) {
        self.open_with(page, None);
    }

    pub fn open_with(self: &Rc<Self>, page: Option<&str>, argument: Option<&str>) {
        let index = page.and_then(pages::index_of);
        let current_view = self.view.borrow().clone();
        let view = match current_view {
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
            self.context.argument.take();
            view.open_nested(subpage.id);
            self.context.argument.replace(argument.map(str::to_owned));
        }
        view.window.present();
    }

    pub fn close(&self) {
        if let Some(view) = self.view.take() {
            view.window.destroy();
        }
        self.context.shared.take();
        self.context.dialog.take();
        self.context.services.updates.cancel();
    }

    pub fn refont(&self) {
        if let Some(view) = self.view.borrow().as_ref() {
            text::refont(view.window.upcast_ref());
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
        let (titlebar, title_holder, tools) = self.titlebar();
        column.append(&titlebar);
        let arrange = move || {
            titlebar.set_visible(config::value_bool("/windows/showTitlebar", true));
            titlebar.set_start_widget(gtk4::Widget::NONE);
            titlebar.set_center_widget(gtk4::Widget::NONE);
            if config::value_bool("/windows/centerTitle", true) {
                title_holder.set_margin_start(0);
                titlebar.set_center_widget(Some(&title_holder));
            } else {
                title_holder.set_margin_start(TITLE_START);
                titlebar.set_start_widget(Some(&title_holder));
            }
        };
        arrange();
        let layout_watch = watch::config("/windows", arrange);

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
        let (header, header_title, back_button) = self.subpage_header();
        self.context.heading.set(Some(&header_title));
        content.append(&header);
        let stage = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        stage.set_vexpand(true);
        content.append(&stage);
        row.append(&content);
        column.append(&row);
        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&column));
        self.context.overlay.set(Some(&overlay));
        window.set_child(Some(&overlay));

        let view = Rc::new(View {
            window: window.clone(),
            context: self.context.clone(),
            rail: rail.clone(),
            stage,
            header,
            header_title,
            page: RefCell::new(None),
            loaded_id: Cell::new(None),
            target: Cell::new(Shown {
                id: PAGES[0].id,
                subpage: None,
            }),
            history: RefCell::new(Vec::new()),
            pending_hit: RefCell::new(None),
            switch_start: Cell::new(0),
            is_switching: Cell::new(false),
            tips: RefCell::new(Vec::new()),
            highlight_watch: RefCell::new(None),
            _layout_watch: layout_watch,
        });
        let highlight_watch = self.add_page_tools(&tools, &view);
        view.highlight_watch.replace(Some(highlight_watch));
        self.context.on_open_subpage.replace(Some(Rc::new({
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
        let go_back: Rc<dyn Fn()> = Rc::new({
            let view = Rc::downgrade(&view);
            move || {
                if let Some(view) = view.upgrade() {
                    view.back();
                }
            }
        });
        self.context.on_back.replace(Some(go_back.clone()));
        view.load();
        back_button.connect_clicked(move |_| go_back());

        rail.connect_selected({
            let view = Rc::downgrade(&view);
            move |page| {
                if let Some(view) = view.upgrade() {
                    view.select(page);
                }
            }
        });
        rail.connect_found({
            let view = Rc::downgrade(&view);
            move |hit| {
                if let Some(view) = view.upgrade() {
                    view.reveal(hit);
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

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let settings = Rc::downgrade(self);
            let view = Rc::downgrade(&view);
            move |key_controller, key, _, modifiers| {
                let (Some(settings), Some(view)) = (settings.upgrade(), view.upgrade()) else {
                    return glib::Propagation::Proceed;
                };
                if key == gdk::Key::Escape {
                    let dialog = view.context.dialog.borrow().clone();
                    match dialog {
                        Some(dialog) => dialog.dismiss(),
                        None => settings.close(),
                    }
                    return glib::Propagation::Stop;
                }
                if view.context.dialog.borrow().is_some() {
                    return glib::Propagation::Proceed;
                }
                if starts_search(key_controller, key, modifiers)
                    && view.rail.type_into_search(key_controller)
                {
                    return glib::Propagation::Stop;
                }
                if modifiers != gdk::ModifierType::CONTROL_MASK {
                    return glib::Propagation::Proceed;
                }
                let current_page = view.rail.current_page();
                let last_page = PAGES.len() - 1;
                let next_page = match key {
                    gdk::Key::Page_Down => (current_page + 1).min(last_page),
                    gdk::Key::Page_Up => current_page.saturating_sub(1),
                    gdk::Key::Tab => (current_page + 1) % PAGES.len(),
                    gdk::Key::ISO_Left_Tab => (current_page + last_page) % PAGES.len(),
                    _ => return glib::Propagation::Proceed,
                };
                view.select(next_page);
                glib::Propagation::Stop
            }
        });
        window.add_controller(key_controller);

        window.connect_close_request({
            let settings = Rc::downgrade(self);
            move |_| {
                if let Some(settings) = settings.upgrade() {
                    settings.view.replace(None);
                    settings.context.shared.take();
                    settings.context.dialog.take();
                }
                glib::Propagation::Proceed
            }
        });
        view
    }

    fn titlebar(self: &Rc<Self>) -> (gtk4::CenterBox, Centered, gtk4::Box) {
        let bar = gtk4::CenterBox::new();
        let title = gtk4::Label::new(Some(&tr("Settings")));
        text::set_font(&title, Family::Title, pixel_size::TITLE as f64, "wght=550");
        text::set_color(&title, "colOnLayer0");
        let title_holder = Centered::new(&title);
        title_holder.set_vexpand(true);

        let close_button = RippleButton::new(&self.context.theme);
        close_button.set_radius(rounding::FULL as f64);
        close_button.set_size_request(CLOSE_SIZE, CLOSE_SIZE);
        close_button.set_valign(gtk4::Align::Center);
        let icon = text::symbol("close", CLOSE_ICON);
        close_button.set_content(&Centered::integral(&icon), 0, 0);
        close_button.connect_clicked({
            let settings = Rc::downgrade(self);
            move |_| {
                if let Some(settings) = settings.upgrade() {
                    settings.close();
                }
            }
        });
        let tools = gtk4::Box::new(gtk4::Orientation::Horizontal, TOOL_SPACING);
        tools.append(&close_button);
        bar.set_end_widget(Some(&tools));
        (bar, title_holder, tools)
    }

    fn round_button(&self, icon: &str) -> (RippleButton, gtk4::Label) {
        let button = RippleButton::new(&self.context.theme);
        button.set_radius(rounding::FULL as f64);
        button.set_size_request(CLOSE_SIZE, CLOSE_SIZE);
        button.set_valign(gtk4::Align::Center);
        let symbol = text::symbol(icon, CLOSE_ICON);
        button.set_content(&Centered::integral(&symbol), 0, 0);
        (button, symbol)
    }

    fn add_page_tools(&self, tools: &gtk4::Box, view: &Rc<View>) -> watch::Watch {
        let (highlight_button, highlight_icon) = self.round_button("ink_highlighter");
        highlight_button.connect_clicked(|_| {
            let is_on = config::value_bool(HIGHLIGHT_CHANGED, false);
            config::store_value(HIGHLIGHT_CHANGED, Value::Bool(!is_on));
        });
        let tip = Tooltip::new(
            &highlight_button,
            &self.context.theme,
            tooltip::Kind::Styled,
        );
        tip.set_text(&tr("Highlight changed settings"));
        tooltip::hover_delay(&highlight_button, &tip, 0);
        let show = {
            let icon = highlight_icon.downgrade();
            move || {
                if let Some(icon) = icon.upgrade() {
                    let is_on = config::value_bool(HIGHLIGHT_CHANGED, false);
                    text::set_symbol_font(&icon, CLOSE_ICON, if is_on { 1.0 } else { 0.0 });
                    text::set_color(&icon, if is_on { "colPrimary" } else { "colOnLayer0" });
                }
            }
        };
        show();
        let highlight_watch = watch::config(HIGHLIGHT_CHANGED, show);

        let (reset_button, _) = self.round_button("settings_backup_restore");
        let reset_tip = Tooltip::new(&reset_button, &self.context.theme, tooltip::Kind::Styled);
        reset_tip.set_text(&tr("Reset this page to defaults"));
        tooltip::hover_delay(&reset_button, &reset_tip, 0);
        reset_button.connect_clicked({
            let view = Rc::downgrade(view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.confirm_reset();
                }
            }
        });
        tools.prepend(&reset_button);
        tools.prepend(&highlight_button);
        view.tips.replace(vec![tip, reset_tip]);
        highlight_watch
    }

    fn subpage_header(&self) -> (gtk4::Box, gtk4::Label, RippleButton) {
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, HEADER_SPACING);
        header.set_margin_top(HEADER_MARGIN);
        header.set_margin_bottom(HEADER_MARGIN);
        header.set_margin_start(HEADER_MARGIN);
        header.set_margin_end(HEADER_MARGIN);
        header.set_visible(false);
        let back_button = RippleButton::new(&self.context.theme);
        back_button.set_radius(rounding::FULL as f64);
        back_button.set_size_request(CLOSE_SIZE, CLOSE_SIZE);
        let icon = text::symbol("arrow_back", CLOSE_ICON);
        back_button.set_content(&Centered::integral(&icon), 0, 0);
        header.append(&back_button);
        let title = text::styled("");
        text::set_font(&title, Family::Main, pixel_size::LARGER as f64, "wght=450");
        text::set_color(&title, "colOnLayer1");
        title.set_xalign(0.0);
        let title_holder = Centered::filling_width(&title);
        title_holder.set_hexpand(true);
        header.append(&title_holder);
        (header, title, back_button)
    }
}

fn focus_on_its_workspace() -> bool {
    let own_pid = std::process::id() as u64;
    let address = hypr::json("clients").and_then(|clients| {
        clients.as_array()?.iter().find_map(|client| {
            let is_own_window = client.get("pid").and_then(Value::as_u64) == Some(own_pid)
                && client.get("title").and_then(Value::as_str) == Some(TITLE);
            is_own_window.then(|| client.get("address")?.as_str().map(str::to_owned))?
        })
    });
    let Some(address) = address else {
        return false;
    };
    hypr::focus_window(&address);
    true
}

fn focus_once_mapped() {
    let try_count = Cell::new(0);
    glib::timeout_add_local(FOCUS_POLL, move || {
        try_count.set(try_count.get() + 1);
        if focus_on_its_workspace() || try_count.get() >= FOCUS_TRIES {
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
}

fn starts_search(
    key_controller: &gtk4::EventControllerKey,
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> bool {
    let command_modifiers = gdk::ModifierType::CONTROL_MASK
        | gdk::ModifierType::ALT_MASK
        | gdk::ModifierType::SUPER_MASK
        | gdk::ModifierType::META_MASK;
    if modifiers.intersects(command_modifiers) {
        return false;
    }
    let is_printable = key
        .to_unicode()
        .is_some_and(|c| !c.is_control() && !c.is_whitespace());
    if !is_printable {
        return false;
    }
    let focus = key_controller
        .widget()
        .and_then(|window| window.root())
        .and_then(|root| root.focus());
    !focus.is_some_and(|focus| {
        focus.is::<gtk4::Text>() || focus.is::<gtk4::TextView>() || focus.is::<gtk4::Editable>()
    })
}

impl View {
    fn select(self: &Rc<Self>, page: usize) {
        self.rail.set_current_page(page);
        self.show(Shown {
            id: PAGES[page].id,
            subpage: None,
        });
    }

    fn reveal(self: &Rc<Self>, hit: &Hit) {
        self.context.argument.take();
        self.pending_hit
            .replace(hit.is_setting.then(|| hit.clone()));
        match pages::subpage(hit.page) {
            Some(subpage) => self.open_nested(subpage.id),
            None => {
                if let Some(index) = pages::index_of(hit.page) {
                    self.select(index);
                }
            }
        }
        if !self.is_switching.get() {
            self.reveal_pending();
        }
    }

    fn reveal_pending(&self) {
        let Some(hit) = self.pending_hit.take() else {
            return;
        };
        if let Some(page) = self.page.borrow().as_ref() {
            let path: Vec<String> = hit.path.iter().map(|part| tr(part)).collect();
            let path: Vec<&str> = path.iter().map(String::as_str).collect();
            page.reveal(&path, &hit.title);
        }
    }

    fn confirm_reset(self: &Rc<Self>) {
        let Some(page) = self.page.borrow().clone() else {
            return;
        };
        let theme = &self.context.theme;
        let changed_count = page.changed_settings();
        let dialog = WindowDialog::new(theme, None);
        dialog.set_background_width(RESET_WIDTH);
        dialog
            .column
            .add(&windowdialog::title(&tr("Reset this page?")), Place::wide());
        let message = match changed_count {
            0 => tr("Every setting on this page that the shell keeps is already at its default"),
            _ => trn(
                "%1 setting on this page goes back to its default. Settings kept by Hyprland or the system stay as they are",
                "%1 settings on this page go back to their defaults. Settings kept by Hyprland or the system stay as they are",
                changed_count as u64,
            ),
        };
        let description = text::styled(&message);
        text::set_color(&description, "colOnSurfaceVariant");
        description.set_wrap(true);
        description.set_xalign(0.0);
        dialog.column.add(&description, Place::wide());
        let (buttons, place) = windowdialog::button_row();
        buttons.append(&windowdialog::spacer());
        let cancel_button = windowdialog::button(theme, &tr("Cancel"));
        cancel_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        buttons.append(&cancel_button);
        let reset_button = windowdialog::button(theme, &tr("Reset"));
        reset_button.set_sensitive(changed_count > 0);
        reset_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let page = Rc::downgrade(&page);
            move |_| {
                if let Some(page) = page.upgrade() {
                    page.reset_to_defaults();
                }
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        buttons.append(&reset_button);
        dialog.column.add(&buttons, place);
        (self.context.dialog_presenter())(dialog);
    }

    fn back(self: &Rc<Self>) {
        let previous = self.history.borrow_mut().pop();
        match previous {
            Some(previous) => self.show_from(previous, false),
            None => self.select(self.rail.current_page()),
        }
    }

    fn open_nested(self: &Rc<Self>, id: &'static str) {
        let chain = pages::ancestors(id);
        if let Some(root) = chain.first().and_then(|root| pages::index_of(root)) {
            self.select(root);
        }
        self.history
            .replace(chain.into_iter().map(Shown::of).collect());
        self.show_from(Shown::of(id), false);
    }

    fn show(self: &Rc<Self>, target: Shown) {
        self.show_from(target, true);
    }

    fn show_from(self: &Rc<Self>, target: Shown, should_remember: bool) {
        if should_remember {
            let current = self.target.get();
            let mut history = self.history.borrow_mut();
            match target.subpage {
                Some(_) if current.id != target.id => history.push(current),
                Some(_) => {}
                None => history.clear(),
            }
        }
        self.header.set_visible(target.subpage.is_some());
        self.header_title
            .set_text(&target.subpage.map(tr).unwrap_or_default());
        let previous = self.target.replace(target);
        if previous.id == target.id && self.loaded_id.get() == Some(target.id) {
            return;
        }
        self.switch_start.set(0);
        if self.is_switching.replace(true) {
            return;
        }
        let view = Rc::downgrade(self);
        self.stage.add_tick_callback(move |stage, clock| {
            let Some(view) = view.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            if view.switch_start.get() == 0 {
                view.switch_start.set(now);
            }
            let elapsed_micros = (now - view.switch_start.get()) as f64;
            if elapsed_micros < SWITCH_OUT {
                stage.set_margin_top(0);
                stage.set_opacity(1.0 - EMPHASIZED.at(elapsed_micros / SWITCH_OUT));
                return glib::ControlFlow::Continue;
            }
            if view.loaded_id.get() != Some(view.target.get().id) {
                view.load();
            }
            let part = ((elapsed_micros - SWITCH_OUT) / SWITCH_IN).min(1.0);
            let eased_progress = EMPHASIZED_LAST_HALF.at(part);
            stage.set_opacity(eased_progress);
            stage.set_margin_top((SWITCH_DROP * (1.0 - eased_progress)).round() as i32);
            if part < 1.0 {
                return glib::ControlFlow::Continue;
            }
            view.is_switching.set(false);
            glib::ControlFlow::Break
        });
    }

    fn load(&self) {
        if let Some(root) = self.stage.root() {
            let has_focus_inside = root
                .focus()
                .is_some_and(|focus| focus.is_ancestor(&self.stage));
            if has_focus_inside {
                root.set_focus(None::<&gtk4::Widget>);
            }
        }
        if let Some(previous) = self.page.take() {
            self.stage.remove(&previous.root);
        }
        let target = self.target.get();
        let keeps_shared = self
            .context
            .shared
            .borrow()
            .as_ref()
            .is_some_and(|(owner, _)| pages::is_within(target.id, owner));
        if !keeps_shared {
            self.context.shared.take();
        }
        let page = pages::build(target.id, target.subpage, &self.context);
        self.stage.append(&page.root);
        self.page.replace(Some(page));
        self.loaded_id.set(Some(target.id));
        self.reveal_pending();
    }
}
