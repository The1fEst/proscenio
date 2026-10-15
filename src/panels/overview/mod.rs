pub mod grid;
pub mod launcher;
mod rows;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::i18n::tr;
use crate::core::scope::Scope;
use crate::panels::overview::launcher::{Item, Launcher};
use crate::platform::hyprland::hypr::Events;
use crate::platform::wayland::grab;
use crate::ui::anim::{EXPRESSIVE_DEFAULT, Tween};
use crate::ui::shapes::{self, Shape};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use grid::Grid;
use rows::Row;

const NAMESPACE: &str = "proscenio:overview";
const ELEVATION: i32 = 10;
const SEARCH_WIDTH: f64 = 360.0;
const SEARCH_WIDTH_COLLAPSED: f64 = 210.0;
const BAR_PADDING: i32 = 4;
const SHAPE_PADDING: i32 = 6;
const LIST_MAX: i32 = 600;
const LIST_MARGIN: i32 = 10;
const LIST_SPACING: i32 = 2;
const RESULT_LIMIT: usize = 15;
const WIDTH_MILLIS: f64 = 300.0;
const HEIGHT_MILLIS: f64 = 500.0;
const GRID_GAP: i32 = ELEVATION * 2 + BAR_PADDING * 2 - 8;

pub struct Overview {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    launcher: Launcher,
    config: Rc<Config>,
    theme: SharedTheme,
    holder: gtk4::Box,
    entry: gtk4::Entry,
    shape: Rc<Cell<Shape>>,
    shape_area: gtk4::DrawingArea,
    symbol: gtk4::Label,
    rule: gtk4::Box,
    scroller: gtk4::ScrolledWindow,
    list: gtk4::Box,
    rows: RefCell<Vec<Rc<Row>>>,
    current_index: Cell<usize>,
    is_list_focused: Cell<bool>,
    is_opened_with_prefix: Cell<bool>,
    should_animate_width: Cell<bool>,
    is_list_shown: Cell<bool>,
    width: Cell<Tween>,
    height: Cell<Tween>,
    is_ticking: Cell<bool>,
    results: RefCell<Vec<Item>>,
    highlight_query: RefCell<String>,
    clipboard: Rc<RefCell<String>>,
    decode_folder: PathBuf,
    grid: Rc<Grid>,
}

impl Overview {
    pub fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide();
            return;
        }
        self.open();
    }

    pub fn open(self: &Rc<Self>) {
        if !self.is_opened_with_prefix.get() {
            self.cancel_search();
        }
        self.launcher.load();
        self.read_clipboard();
        self.window.set_visible(true);
        self.show_grid(self.query().is_empty());
        self.focus_entry();
        self.run();
        let (Some(grab), Some(surface)) = (self.grab.as_ref(), self.window.surface()) else {
            return;
        };
        let overview = Rc::downgrade(self);
        grab.hold(&surface, move || {
            if let Some(overview) = overview.upgrade() {
                overview.hide();
            }
        });
    }

    pub fn hide(&self) {
        self.should_animate_width.set(false);
        self.is_opened_with_prefix.set(false);
        if let Some(grab) = self.grab.as_ref() {
            grab.release();
        }
        self.window.set_visible(false);
        self.grid.close();
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.rows.replace(Vec::new());
        self.results.replace(Vec::new());
        self.launcher.release();
        self.clipboard.replace(String::new());
        let _ = std::fs::remove_dir_all(&self.decode_folder);
    }

    fn read_clipboard(&self) {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let text = self.clipboard.clone();
        display
            .clipboard()
            .read_text_async(gio::Cancellable::NONE, move |result| {
                if let Ok(Some(value)) = result {
                    text.replace(value.to_string());
                }
            });
    }

    pub fn toggle_clipboard(self: &Rc<Self>) {
        let prefix = config::current().search_clipboard.clone();
        self.toggle_prefixed(&prefix);
    }

    pub fn toggle_emojis(self: &Rc<Self>) {
        let prefix = config::current().search_emojis.clone();
        self.toggle_prefixed(&prefix);
    }

    fn toggle_prefixed(self: &Rc<Self>, prefix: &str) {
        if self.window.is_visible() && self.is_opened_with_prefix.get() {
            self.hide();
            return;
        }
        self.is_opened_with_prefix.set(true);
        self.set_searching_text(prefix);
        self.select(0);
        if !self.window.is_visible() {
            self.open();
        }
        let entry = self.entry.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(entry) = entry.upgrade() {
                entry.set_position(-1);
            }
        });
    }

    fn cancel_search(&self) {
        self.entry.select_region(0, -1);
        self.launcher.set_query("");
        self.should_animate_width.set(true);
    }

    fn set_searching_text(&self, text: &str) {
        self.entry.set_text(text);
        self.entry.set_position(-1);
        self.launcher.set_query(text);
    }

    fn focus_entry(&self) {
        self.is_list_focused.set(false);
        self.entry.grab_focus_without_selecting();
    }

    fn focus_list(self: &Rc<Self>) {
        if self.rows.borrow().is_empty() {
            return;
        }
        self.is_list_focused.set(true);
        gtk4::prelude::GtkWindowExt::set_focus(&self.window, None::<&gtk4::Widget>);
        self.select(1);
    }

    fn select(self: &Rc<Self>, index: usize) {
        let has_grown = index >= self.rows.borrow().len() && self.grow(index + RESULT_LIMIT);
        let rows = self.rows.borrow();
        if rows.is_empty() {
            self.current_index.set(0);
            return;
        }
        let index = index.min(rows.len() - 1);
        self.current_index.set(index);
        let row = rows[index].button.clone();
        drop(rows);
        self.repaint();
        if !has_grown {
            self.scroll_to(&row);
            return;
        }
        let overview = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(overview) = overview.upgrade() {
                overview.scroll_to(&row);
            }
        });
    }

    fn repaint(&self) {
        let current_index = self.current_index.get();
        for (position, row) in self.rows.borrow().iter().enumerate() {
            row.paint(position == current_index || row.is_hovered.get());
        }
    }

    fn grow(self: &Rc<Self>, count: usize) -> bool {
        let row_count = self.rows.borrow().len();
        let items: Vec<Item> = self
            .results
            .borrow()
            .iter()
            .skip(row_count)
            .take(count.saturating_sub(row_count))
            .cloned()
            .collect();
        if items.is_empty() {
            return false;
        }
        let query = self.highlight_query.borrow().clone();
        for item in items {
            let row = self.row(item, &query);
            self.list.append(&row.widget);
            self.rows.borrow_mut().push(row);
        }
        true
    }

    fn scroll_to(&self, row: &RippleButton) {
        let Some(bounds) = row.compute_bounds(&self.list) else {
            return;
        };
        let adjustment = self.scroller.vadjustment();
        let top = bounds.y() as f64;
        let bottom = top + bounds.height() as f64;
        let page = adjustment.page_size();
        if top < adjustment.value() {
            adjustment.set_value(top);
        } else if bottom > adjustment.value() + page {
            adjustment.set_value(bottom - page);
        }
    }

    fn activate(&self, index: usize) {
        let item = self.rows.borrow().get(index).map(|row| row.item.clone());
        if let Some(item) = item {
            self.hide();
            self.launcher.execute(&item.run);
        }
    }

    fn complete(&self, index: usize) {
        let name = self
            .rows
            .borrow()
            .get(index)
            .map(|row| row.item.name.clone());
        if let Some(name) = name {
            self.launcher.set_query(&name);
            self.entry.set_text(&name);
            self.entry.set_position(-1);
            self.focus_entry();
        }
    }

    fn delete_current(&self) {
        let delete = tr("Delete");
        let run = self
            .rows
            .borrow()
            .get(self.current_index.get())
            .and_then(|row| {
                row.item
                    .actions
                    .iter()
                    .find(|action| action.name == delete)
                    .map(|action| action.run.clone())
            });
        if let Some(run) = run {
            self.launcher.execute(&run);
        }
    }

    fn type_anywhere(self: &Rc<Self>, key: gdk::Key, is_control_held: bool) -> bool {
        if key == gdk::Key::BackSpace {
            self.focus_entry();
            let text: Vec<char> = self.entry.text().chars().collect();
            let position = (self.entry.position().max(0) as usize).min(text.len());
            if position > 0 {
                let length = if is_control_held {
                    let left: String = text[..position].iter().collect();
                    let head = left.trim_end();
                    let word_start = head.rfind(char::is_whitespace).map_or(0, |at| {
                        at + head[at..].chars().next().map_or(1, char::len_utf8)
                    });
                    left[word_start..].chars().count()
                } else {
                    1
                };
                let remaining_text: String = text[..position - length]
                    .iter()
                    .chain(text[position..].iter())
                    .collect();
                self.entry.set_text(&remaining_text);
            }
            self.entry.set_position(-1);
            return true;
        }
        let Some(letter) = key.to_unicode() else {
            return false;
        };
        if (letter as u32) < 0x20 || letter == '\u{7f}' {
            return false;
        }
        self.focus_entry();
        let mut position = self.entry.position();
        self.entry.insert_text(&letter.to_string(), &mut position);
        self.entry.set_position(position);
        self.select(0);
        true
    }

    fn query(&self) -> String {
        self.launcher.query.borrow().clone()
    }

    fn refresh_bar(self: &Rc<Self>) {
        let query = self.query();
        let (shape, icon) = prefix_look(&config::current(), &query);
        self.shape.set(shape);
        self.shape_area.queue_draw();
        self.symbol.set_text(icon);
        self.show_grid(query.is_empty());
        let target = if query.is_empty() {
            SEARCH_WIDTH_COLLAPSED
        } else {
            SEARCH_WIDTH
        };
        let mut width = self.width.get();
        if self.should_animate_width.get() && self.window.is_mapped() {
            width.retarget(target, now(&self.holder));
        } else {
            width.jump(target);
        }
        self.width.set(width);
        self.run();
    }

    fn show_grid(&self, is_query_empty: bool) {
        let is_shown = is_query_empty && self.config.overview_enable;
        self.grid.widget.set_visible(is_shown);
        if is_shown && self.window.is_visible() {
            self.grid.open();
        } else {
            self.grid.close();
        }
    }

    fn fill(self: &Rc<Self>) {
        let query = self.query();
        self.highlight_query
            .replace(self.launcher.clean_one_prefix(&query).to_owned());
        self.results.replace(self.launcher.results());
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.rows.replace(Vec::new());
        self.grow(RESULT_LIMIT);
        let current_index = self.current_index.get();
        self.select(current_index);
        self.resize();
    }

    fn resize(self: &Rc<Self>) {
        let is_shown = !self.query().is_empty();
        let was_shown = self.is_list_shown.replace(is_shown);
        self.rule.set_visible(is_shown);
        self.scroller.set_visible(is_shown);
        let width = self.list.width().max(SEARCH_WIDTH as i32);
        let natural_height = self.list.measure(gtk4::Orientation::Vertical, width).1;
        let target = natural_height.min(LIST_MAX) as f64;
        let mut height = self.height.get();
        if is_shown && self.window.is_visible() {
            if !was_shown {
                height.jump(0.0);
            }
            height.retarget(target, now(&self.holder));
        } else {
            height.jump(if is_shown { target } else { 0.0 });
        }
        self.height.set(height);
        self.run();
    }

    fn on_results_changed(self: &Rc<Self>) {
        self.refresh_bar();
        self.current_index.set(0);
        self.fill();
    }

    fn grow_near_end(self: &Rc<Self>) {
        let adjustment = self.scroller.vadjustment();
        let page = adjustment.page_size();
        if adjustment.value() + page * 2.0 < adjustment.upper() {
            return;
        }
        let row_count = self.rows.borrow().len();
        self.grow(row_count + RESULT_LIMIT);
    }

    fn run(self: &Rc<Self>) {
        self.apply(now(&self.holder));
        if self.is_ticking.replace(true) {
            return;
        }
        let overview = Rc::downgrade(self);
        self.holder.add_tick_callback(move |_, clock| {
            let Some(overview) = overview.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            overview.apply(now);
            if overview.width.get().is_running(now) || overview.height.get().is_running(now) {
                return glib::ControlFlow::Continue;
            }
            overview.is_ticking.set(false);
            overview.mask();
            glib::ControlFlow::Break
        });
    }

    fn apply(&self, now: i64) {
        self.entry
            .set_size_request(self.width.get().value(now).round() as i32, -1);
        self.scroller
            .set_size_request(-1, self.height.get().value(now).round().max(0.0) as i32);
        self.mask();
    }

    fn mask(&self) {
        let Some(surface) = self.window.surface() else {
            return;
        };
        let Some(bounds) = self.holder.compute_bounds(&self.window) else {
            return;
        };
        let region = gtk4::cairo::RectangleInt::new(
            bounds.x() as i32,
            bounds.y() as i32,
            bounds.width().ceil() as i32,
            bounds.height().ceil() as i32,
        );
        surface.set_input_region(Some(&gtk4::cairo::Region::create_rectangle(&region)));
    }
}

fn prefix_look(config: &Config, query: &str) -> (Shape, &'static str) {
    if query.starts_with(config.search_action.as_str()) {
        return (Shape::Pill, "settings_suggest");
    }
    if query.starts_with(config.search_app.as_str()) {
        return (Shape::Clover4Leaf, "apps");
    }
    if query.starts_with(config.search_clipboard.as_str()) {
        return (Shape::Gem, "content_paste_search");
    }
    if query.starts_with(config.search_emojis.as_str()) {
        return (Shape::Sunny, "add_reaction");
    }
    if query.starts_with(config.search_math.as_str()) {
        return (Shape::PuffyDiamond, "calculate");
    }
    if query.starts_with(config.search_shell.as_str()) {
        return (Shape::PixelCircle, "terminal");
    }
    (Shape::Cookie7Sided, "search")
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}

pub fn build(
    app: &gtk4::Application,
    config: &Rc<Config>,
    launcher: &Launcher,
    theme: &SharedTheme,
    events: &Events,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Rc<Overview> {
    let shape = Rc::new(Cell::new(Shape::Cookie7Sided));
    let symbol = text::symbol("search", pixel_size::HUGE as f64);
    text::set_color(&symbol, "colOnSecondaryContainer");
    let symbol_holder = Centered::new(&symbol);
    let (_, symbol_width, _, _) = symbol_holder.measure(gtk4::Orientation::Horizontal, -1);
    let (_, symbol_height, _, _) = symbol_holder.measure(gtk4::Orientation::Vertical, -1);
    let shape_size = symbol_width.max(symbol_height) + SHAPE_PADDING * 2;
    let shape_area = gtk4::DrawingArea::new();
    shape_area.set_content_width(shape_size);
    shape_area.set_content_height(shape_size);
    shape_area.set_draw_func({
        let theme = theme.clone();
        let shape = shape.clone();
        move |_, cr, width, height| {
            let size = width.min(height) as f64;
            let color = theme.borrow().colors.col_secondary_container;
            cr.set_source_rgba(
                color.red() as f64,
                color.green() as f64,
                color.blue() as f64,
                color.alpha() as f64,
            );
            shapes::polygon(shape.get()).trace(cr, 0.0, 0.0, size);
            let _ = cr.fill();
        }
    });
    let shape_holder = gtk4::Overlay::new();
    shape_holder.set_child(Some(&shape_area));
    shape_holder.add_overlay(&symbol_holder);
    shape_holder.set_valign(gtk4::Align::Center);

    let entry = gtk4::Entry::new();
    entry.add_css_class("search-input");
    entry.set_has_frame(false);
    entry.set_placeholder_text(Some(&tr("Search, calculate or run")));
    entry.set_margin_top(BAR_PADDING);
    entry.set_margin_bottom(BAR_PADDING);
    entry.set_valign(gtk4::Align::Center);
    entry.set_attributes(&{
        let attributes = pango::AttrList::new();
        attributes.insert(pango::AttrFontDesc::new(&text::font(
            Family::Main,
            pixel_size::SMALL as f64,
            "wght=450",
        )));
        attributes
    });

    let filler = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    filler.set_hexpand(true);

    let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    bar.set_margin_start(10);
    bar.set_margin_end(4);
    bar.set_margin_top(BAR_PADDING);
    bar.set_margin_bottom(BAR_PADDING);
    bar.append(&shape_holder);
    bar.append(&entry);
    bar.append(&filler);

    let rule = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    rule.add_css_class("search-rule");
    rule.set_size_request(-1, 1);
    rule.set_visible(false);

    let list = gtk4::Box::new(gtk4::Orientation::Vertical, LIST_SPACING);
    list.set_margin_top(LIST_MARGIN);
    list.set_margin_bottom(LIST_MARGIN);
    list.set_valign(gtk4::Align::Start);

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroller.set_child(Some(&list));
    crate::ui::widgets::input::flickable::follow_scroll_settings(&scroller);
    scroller.set_visible(false);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.add_css_class("search-panel");
    column.set_overflow(gtk4::Overflow::Hidden);
    column.set_halign(gtk4::Align::Center);
    column.append(&bar);
    column.append(&rule);
    column.append(&scroller);

    let connector: String = monitor.connector().map(Into::into).unwrap_or_default();
    let grid = grid::build(config, theme, events, &connector, scope);
    grid.widget.set_margin_top(GRID_GAP);

    let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    holder.set_halign(gtk4::Align::Center);
    holder.set_valign(gtk4::Align::Start);
    holder.set_margin_top(ELEVATION);
    holder.set_margin_bottom(ELEVATION);
    holder.set_margin_start(ELEVATION);
    holder.set_margin_end(ELEVATION);
    holder.append(&column);
    holder.append(&grid.widget);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&holder)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_visible(false);

    let overview = Rc::new(Overview {
        window: window.clone(),
        grab: grab::Grab::new(&monitor.display()),
        launcher: launcher.clone(),
        config: config.clone(),
        theme: theme.clone(),
        holder: holder.clone(),
        entry: entry.clone(),
        shape,
        shape_area,
        symbol,
        rule,
        scroller,
        list,
        rows: RefCell::new(Vec::new()),
        current_index: Cell::new(0),
        is_list_focused: Cell::new(false),
        is_opened_with_prefix: Cell::new(false),
        should_animate_width: Cell::new(false),
        is_list_shown: Cell::new(false),
        width: Cell::new(Tween::new(
            SEARCH_WIDTH_COLLAPSED,
            WIDTH_MILLIS,
            EXPRESSIVE_DEFAULT,
        )),
        height: Cell::new(Tween::new(0.0, HEIGHT_MILLIS, EXPRESSIVE_DEFAULT)),
        is_ticking: Cell::new(false),
        results: RefCell::new(Vec::new()),
        highlight_query: RefCell::new(String::new()),
        clipboard: Rc::new(RefCell::new(String::new())),
        decode_folder: glib::user_runtime_dir().join("proscenio/cliphist"),
        grid,
    });
    overview.grid.connect_close({
        let overview = Rc::downgrade(&overview);
        move || {
            if let Some(overview) = overview.upgrade() {
                overview.hide();
            }
        }
    });
    overview.apply(0);

    let _ = std::fs::remove_dir_all(&overview.decode_folder);

    if let Some(display) = gdk::Display::default() {
        let clipboard = display.clipboard();
        let handler = clipboard.connect_changed({
            let overview = Rc::downgrade(&overview);
            move |_| {
                if let Some(overview) = overview.upgrade()
                    && overview.window.is_visible()
                {
                    overview.read_clipboard();
                }
            }
        });
        scope.defer(move || clipboard.disconnect(handler));
    }

    let adjustment = overview.scroller.vadjustment();
    let on_adjustment_changed = {
        let overview = Rc::downgrade(&overview);
        move |_: &gtk4::Adjustment| {
            let overview = overview.clone();
            glib::idle_add_local_once(move || {
                if let Some(overview) = overview.upgrade() {
                    overview.grow_near_end();
                }
            });
        }
    };
    adjustment.connect_value_changed(on_adjustment_changed.clone());
    adjustment.connect_changed(on_adjustment_changed);

    scope.keep(launcher.subscribe({
        let overview = Rc::downgrade(&overview);
        move || {
            if let Some(overview) = overview.upgrade() {
                overview.on_results_changed();
            }
        }
    }));

    entry.connect_changed({
        let launcher = launcher.clone();
        move |entry| launcher.set_query(&entry.text())
    });
    entry.connect_activate({
        let overview = Rc::downgrade(&overview);
        move |_| {
            if let Some(overview) = overview.upgrade() {
                overview.activate(0);
            }
        }
    });

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    key_controller.connect_key_pressed({
        let overview = Rc::downgrade(&overview);
        move |_, key, _, state| {
            let Some(overview) = overview.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let is_control_held = state.contains(gdk::ModifierType::CONTROL_MASK);
            let is_shift_held = state.contains(gdk::ModifierType::SHIFT_MASK);
            if key == gdk::Key::Escape {
                overview.hide();
                return glib::Propagation::Stop;
            }
            if !overview.is_list_focused.get() {
                return match key {
                    gdk::Key::Down => {
                        overview.focus_list();
                        glib::Propagation::Stop
                    }
                    gdk::Key::Tab => {
                        overview.complete(0);
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                };
            }
            let current_index = overview.current_index.get();
            match key {
                gdk::Key::Up if current_index == 0 => overview.focus_entry(),
                gdk::Key::Up => overview.select(current_index - 1),
                gdk::Key::Down => overview.select(current_index + 1),
                gdk::Key::Return | gdk::Key::KP_Enter => overview.activate(current_index),
                gdk::Key::Delete if is_shift_held => overview.delete_current(),
                gdk::Key::Tab => overview.complete(current_index),
                _ if is_control_held && key != gdk::Key::BackSpace => {
                    return glib::Propagation::Proceed;
                }
                _ => {
                    if !overview.type_anywhere(key, is_control_held) {
                        return glib::Propagation::Proceed;
                    }
                }
            }
            glib::Propagation::Stop
        }
    });
    window.add_controller(key_controller);

    window.connect_map({
        let overview = Rc::downgrade(&overview);
        move |_| {
            if let Some(overview) = overview.upgrade() {
                overview.mask();
            }
        }
    });

    overview
}
