pub mod grid;
pub mod launcher;

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
use crate::panels::overview::launcher::{IconType, Item, Launcher, Run};
use crate::platform::grab;
use crate::platform::hypr::Events;
use crate::services::cliphist;
use crate::ui::anim::{EXPRESSIVE_DEFAULT, Tween};
use crate::ui::shapes::{self, Shape};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text::{self, Family};
use crate::ui::widgets::tooltip::{self, Tooltip};
use grid::Grid;

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
const ITEM_MARGIN: i32 = 10;
const ITEM_PADDING: i32 = 10;
const ITEM_VERTICAL_PADDING: i32 = 6;
const APP_ICON: i32 = 35;
const ACTION_BUTTON: i32 = 34;
const ACTION_ICON: i32 = 20;
const PREVIEW_HEIGHT: f64 = 140.0;
const EMOJI_QT_SCALE: f64 = 20.0 / 23.0;
const IMAGE_TOP_LINE: i32 = 24;
const IMAGE_PREVIEW_TOP: i32 = 40;
const IMAGE_SQUEEZE: i32 = 6;
const BLUR_ICON: f64 = 28.0;
const GRID_GAP: i32 = ELEVATION * 2 + BAR_PADDING * 2 - 8;

struct Row {
    item: Item,
    widget: gtk4::Overlay,
    button: RippleButton,
    kind: gtk4::Label,
    name: gtk4::Label,
    glyph: Option<gtk4::Label>,
    verb: gtk4::Label,
    symbols: Vec<gtk4::Label>,
    plain: String,
    highlighted: String,
    hovered: Cell<bool>,
}

impl Row {
    fn paint(&self, selected: bool) {
        let foreground = if selected {
            "colOnPrimaryContainer"
        } else {
            "m3onSurface"
        };
        self.button.set_toggled(selected);
        text::set_color(
            &self.kind,
            if selected {
                "colOnPrimaryContainer"
            } else {
                "colSubtext"
            },
        );
        text::set_color(&self.name, foreground);
        self.name.set_markup(if selected {
            &self.plain
        } else {
            &self.highlighted
        });
        if let Some(glyph) = &self.glyph {
            text::set_color(glyph, foreground);
        }
        for symbol in &self.symbols {
            text::set_color(symbol, foreground);
        }
        self.verb.set_visible(selected);
    }
}

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
    current: Cell<usize>,
    list_focused: Cell<bool>,
    dont_auto_cancel: Cell<bool>,
    animate_width: Cell<bool>,
    showing: Cell<bool>,
    width: Cell<Tween>,
    height: Cell<Tween>,
    ticking: Cell<bool>,
    results: RefCell<Vec<Item>>,
    highlight_query: RefCell<String>,
    clipboard: Rc<RefCell<String>>,
    decode: PathBuf,
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
        if !self.dont_auto_cancel.get() {
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
        let overview = self.clone();
        grab.hold(&surface, move || overview.hide());
    }

    pub fn hide(&self) {
        self.animate_width.set(false);
        self.dont_auto_cancel.set(false);
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
        let _ = std::fs::remove_dir_all(&self.decode);
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
        if self.window.is_visible() && self.dont_auto_cancel.get() {
            self.hide();
            return;
        }
        self.dont_auto_cancel.set(true);
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
        self.animate_width.set(true);
    }

    fn set_searching_text(&self, text: &str) {
        self.entry.set_text(text);
        self.entry.set_position(-1);
        self.launcher.set_query(text);
    }

    fn focus_entry(&self) {
        self.list_focused.set(false);
        self.entry.grab_focus_without_selecting();
    }

    fn focus_list(self: &Rc<Self>) {
        if self.rows.borrow().is_empty() {
            return;
        }
        self.list_focused.set(true);
        gtk4::prelude::GtkWindowExt::set_focus(&self.window, None::<&gtk4::Widget>);
        self.select(1);
    }

    fn select(self: &Rc<Self>, index: usize) {
        let grew = index >= self.rows.borrow().len() && self.grow(index + RESULT_LIMIT);
        let rows = self.rows.borrow();
        if rows.is_empty() {
            self.current.set(0);
            return;
        }
        let index = index.min(rows.len() - 1);
        self.current.set(index);
        let row = rows[index].button.clone();
        drop(rows);
        self.repaint();
        if !grew {
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
        let current = self.current.get();
        for (position, row) in self.rows.borrow().iter().enumerate() {
            row.paint(position == current || row.hovered.get());
        }
    }

    fn grow(self: &Rc<Self>, count: usize) -> bool {
        let built = self.rows.borrow().len();
        let items: Vec<Item> = self
            .results
            .borrow()
            .iter()
            .skip(built)
            .take(count.saturating_sub(built))
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
        let run = self.rows.borrow().get(self.current.get()).and_then(|row| {
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

    fn type_anywhere(self: &Rc<Self>, key: gdk::Key, control: bool) -> bool {
        if key == gdk::Key::BackSpace {
            self.focus_entry();
            let text: Vec<char> = self.entry.text().chars().collect();
            let position = (self.entry.position().max(0) as usize).min(text.len());
            if position > 0 {
                let length = if control {
                    let left: String = text[..position].iter().collect();
                    let trimmed = left.trim_end();
                    let word = trimmed.rfind(char::is_whitespace).map_or(0, |at| {
                        at + trimmed[at..].chars().next().map_or(1, char::len_utf8)
                    });
                    left[word..].chars().count()
                } else {
                    1
                };
                let kept: String = text[..position - length]
                    .iter()
                    .chain(text[position..].iter())
                    .collect();
                self.entry.set_text(&kept);
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
        if self.animate_width.get() && self.window.is_mapped() {
            width.retarget(target, now(&self.holder));
        } else {
            width.jump(target);
        }
        self.width.set(width);
        self.run();
    }

    fn show_grid(&self, empty_query: bool) {
        let shown = empty_query && self.config.overview_enable;
        self.grid.widget.set_visible(shown);
        if shown && self.window.is_visible() {
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
        let current = self.current.get();
        self.select(current);
        self.resize();
    }

    fn resize(self: &Rc<Self>) {
        let shown = !self.query().is_empty();
        let was = self.showing.replace(shown);
        self.rule.set_visible(shown);
        self.scroller.set_visible(shown);
        let width = self.list.width().max(SEARCH_WIDTH as i32);
        let natural = self.list.measure(gtk4::Orientation::Vertical, width).1;
        let target = natural.min(LIST_MAX) as f64;
        let mut height = self.height.get();
        if shown && self.window.is_visible() {
            if !was {
                height.jump(0.0);
            }
            height.retarget(target, now(&self.holder));
        } else {
            height.jump(if shown { target } else { 0.0 });
        }
        self.height.set(height);
        self.run();
    }

    fn results_changed(self: &Rc<Self>) {
        self.refresh_bar();
        self.current.set(0);
        self.fill();
    }

    fn grow_near_end(self: &Rc<Self>) {
        let adjustment = self.scroller.vadjustment();
        let page = adjustment.page_size();
        if adjustment.value() + page * 2.0 < adjustment.upper() {
            return;
        }
        let built = self.rows.borrow().len();
        self.grow(built + RESULT_LIMIT);
    }

    fn run(self: &Rc<Self>) {
        self.apply(now(&self.holder));
        if self.ticking.replace(true) {
            return;
        }
        let overview = Rc::downgrade(self);
        self.holder.add_tick_callback(move |_, clock| {
            let Some(overview) = overview.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            overview.apply(now);
            if overview.width.get().running(now) || overview.height.get().running(now) {
                return glib::ControlFlow::Continue;
            }
            overview.ticking.set(false);
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

    fn row(self: &Rc<Self>, item: Item, query: &str) -> Rc<Row> {
        let raw = item.raw.clone();
        let image = !raw.is_empty() && cliphist::is_image(&raw);
        let monospace = item.monospace;

        let button = RippleButton::new(&self.theme);
        button.set_look(Look {
            background: |theme| {
                crate::ui::theme::transparentize(theme.colors.col_primary_container, 1.0)
            },
            hover: |theme| theme.colors.col_primary_container,
            toggled: |theme| theme.colors.col_primary_container,
            toggled_hover: |theme| theme.colors.col_primary_container,
            ripple: |theme| theme.colors.col_primary_container_active,
            ripple_toggled: |theme| theme.colors.col_primary_container_active,
        });
        button.set_radius(crate::ui::theme::rounding::NORMAL as f64);
        button.set_background_insets(ITEM_MARGIN as f32, 0.0);
        button.set_cursor_from_name(Some("pointer"));
        button.set_can_focus(false);

        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);

        let mut glyph = None;
        match item.icon_type {
            IconType::System => {
                line.append(&app_icon(&item.icon, APP_ICON, &button));
            }
            IconType::Material => {
                let symbol = text::symbol(&item.icon, 30.0);
                line.append(&Centred::new(&symbol));
                glyph = Some(symbol);
            }
            IconType::Text => {
                let label = text::styled_sized(&item.icon, pixel_size::LARGER);
                text::set_font(
                    &label,
                    Family::Main,
                    pixel_size::LARGER as f64 * EMOJI_QT_SCALE,
                    "wght=450",
                );
                label.set_valign(gtk4::Align::Center);
                line.append(&label);
                glyph = Some(label);
            }
            IconType::None => line.set_spacing(0),
        }

        let kind = text::styled_sized(&item.kind, pixel_size::SMALLER);
        kind.set_xalign(0.0);
        let kind_row = gtk4::Overlay::new();
        let kind_holder = Centred::filling_width(&kind);
        kind_holder.set_hexpand(true);
        kind_holder.set_visible(!item.kind.is_empty() && item.kind != tr("App"));
        kind_row.set_child(Some(&kind_holder));

        let mut symbols = Vec::new();
        let mut placed: Vec<(gtk4::Box, gtk4::Box)> = Vec::new();
        if image {
            let actions = self.actions(&item, &mut symbols);
            let place = placeholder(&actions);
            place.set_margin_end(ITEM_VERTICAL_PADDING);
            place.set_halign(gtk4::Align::End);
            place.set_valign(gtk4::Align::Start);
            kind_holder.set_size_request(-1, IMAGE_TOP_LINE);
            kind_row.add_overlay(&place);
            placed.push((actions, place));
        }

        let name = gtk4::Label::new(None);
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_ellipsize(pango::EllipsizeMode::End);
        text::set_font(
            &name,
            if monospace {
                Family::Monospace
            } else {
                Family::Main
            },
            pixel_size::SMALL as f64,
            "wght=450",
        );
        if raw.is_empty() {
            name.set_single_line_mode(true);
        } else {
            name.set_wrap(true);
            name.set_wrap_mode(pango::WrapMode::WordChar);
            name.set_lines(3);
        }
        let plain = glib::markup_escape_text(&item.name).to_string();
        let highlighted = if monospace {
            plain.clone()
        } else {
            highlight(&item.name, query, &self.theme)
        };
        name.set_visible(!image);

        let name_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        if !raw.is_empty() && *self.clipboard.borrow() == item.name {
            name_row.append(&check_mark());
        }
        if raw.is_empty() {
            let holder = Centred::filling_width(&name);
            holder.set_hexpand(true);
            holder.set_visible(!image);
            name_row.append(&holder);
        } else {
            name_row.append(&name);
        }

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        column.set_hexpand(true);
        column.set_valign(gtk4::Align::Center);
        column.append(&kind_row);
        column.append(&name_row);
        let preview = if image {
            let preview = self.preview(&raw, item.blur);
            preview.set_margin_top(IMAGE_PREVIEW_TOP - IMAGE_TOP_LINE);
            preview.set_margin_bottom(ITEM_VERTICAL_PADDING - IMAGE_SQUEEZE);
            preview
        } else {
            let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            spacer.set_margin_bottom(ITEM_VERTICAL_PADDING);
            spacer.upcast()
        };
        column.append(&preview);
        line.append(&column);

        let verb = text::styled_sized(&item.verb, pixel_size::NORMAL);
        text::set_color(&verb, "colOnPrimaryContainer");
        let verb_holder = Centred::new(&verb);
        verb_holder.set_valign(gtk4::Align::Center);
        line.append(&verb_holder);
        verb.set_visible(false);

        if !image && !item.actions.is_empty() {
            let actions = self.actions(&item, &mut symbols);
            let place = placeholder(&actions);
            place.set_valign(gtk4::Align::Center);
            line.append(&place);
            placed.push((actions, place));
        }

        button.set_content(&line, ITEM_MARGIN + ITEM_PADDING, ITEM_VERTICAL_PADDING);

        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&button));
        for (actions, _) in &placed {
            widget.add_overlay(actions);
        }
        widget.connect_get_child_position(move |overlay, child| {
            let (_, place) = placed
                .iter()
                .find(|(actions, _)| actions.upcast_ref::<gtk4::Widget>() == child)?;
            let bounds = place.compute_bounds(overlay)?;
            Some(gdk::Rectangle::new(
                bounds.x().round() as i32,
                bounds.y().round() as i32,
                bounds.width().round() as i32,
                bounds.height().round() as i32,
            ))
        });

        let row = Rc::new(Row {
            item,
            widget: widget.clone(),
            button: button.clone(),
            kind,
            name,
            glyph,
            verb,
            symbols,
            plain,
            highlighted,
            hovered: Cell::new(false),
        });

        let motion = gtk4::EventControllerMotion::new();
        motion.connect_enter({
            let row = Rc::downgrade(&row);
            let overview = Rc::downgrade(self);
            move |_, _, _| {
                let (Some(row), Some(overview)) = (row.upgrade(), overview.upgrade()) else {
                    return;
                };
                row.hovered.set(true);
                overview.repaint();
            }
        });
        motion.connect_leave({
            let row = Rc::downgrade(&row);
            let overview = Rc::downgrade(self);
            move |_| {
                let (Some(row), Some(overview)) = (row.upgrade(), overview.upgrade()) else {
                    return;
                };
                row.hovered.set(false);
                overview.repaint();
            }
        });
        widget.add_controller(motion);

        button.connect_clicked({
            let overview = Rc::downgrade(self);
            let run = row.item.run.clone();
            move |_| {
                if let Some(overview) = overview.upgrade() {
                    overview.hide();
                    overview.launcher.execute(&run);
                }
            }
        });
        row
    }

    fn actions(self: &Rc<Self>, item: &Item, symbols: &mut Vec<gtk4::Label>) -> gtk4::Box {
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        for action in item.actions.iter().take(4) {
            let button = RippleButton::new(&self.theme);
            button.set_look(Look {
                hover: |theme| theme.colors.col_secondary_container_hover,
                ripple: |theme| theme.colors.col_secondary_container_active,
                ..Look::default()
            });
            button.set_size_request(ACTION_BUTTON, ACTION_BUTTON);
            button.set_can_focus(false);
            if action.icon_type == IconType::Material || action.icon.is_empty() {
                let name = if action.icon.is_empty() {
                    "video_settings"
                } else {
                    action.icon.as_str()
                };
                let symbol = text::symbol(name, pixel_size::HUGEASS as f64);
                symbols.push(symbol.clone());
                button.set_content(&Centred::new(&symbol), 0, 0);
            } else {
                button.set_content(&app_icon(&action.icon, ACTION_ICON, &button), 0, 0);
            }
            let tip = Tooltip::new(&button, &self.theme, tooltip::Kind::Styled);
            tip.set_text(&action.name);
            tooltip::hover_delay(&button, &tip, 0);
            button.connect_clicked({
                let launcher = self.launcher.clone();
                let run: Run = action.run.clone();
                move |_| launcher.execute(&run)
            });
            actions.append(&button);
        }
        actions
    }

    fn preview(&self, entry: &str, blur: bool) -> gtk4::Widget {
        let (width, height) = cliphist::image_size(entry).unwrap_or((0, 0));
        let max_width = SEARCH_WIDTH - ((ITEM_MARGIN + ITEM_PADDING) * 2) as f64;
        let scale = if width == 0 || height == 0 {
            0.0
        } else {
            (max_width / width as f64)
                .min(PREVIEW_HEIGHT / height as f64)
                .min(1.0)
        };
        let shown_width = (width as f64 * scale).round() as i32;
        let shown_height = (height as f64 * scale).round() as i32;

        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Contain);
        picture.set_can_shrink(true);
        picture.set_halign(gtk4::Align::Center);
        picture.set_size_request(shown_width, shown_height);

        let frame = gtk4::Overlay::new();
        frame.add_css_class("search-preview");
        frame.set_overflow(gtk4::Overflow::Hidden);
        frame.set_hexpand(true);
        frame.set_size_request(-1, shown_height);
        frame.set_child(Some(&picture));

        if blur {
            picture.add_css_class("search-blurred");
            let veil = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            veil.add_css_class("search-veil");
            veil.set_valign(gtk4::Align::Fill);
            let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            inner.set_valign(gtk4::Align::Center);
            inner.set_vexpand(true);
            let symbol = text::symbol("visibility_off", BLUR_ICON);
            let label = text::styled_sized(&tr("Image hidden"), pixel_size::SMALLIE);
            text::set_color(&label, "colOnSurface");
            inner.append(&Centred::new(&symbol));
            inner.append(&Centred::new(&label));
            veil.append(&inner);
            frame.add_overlay(&veil);
        }

        let number = cliphist::entry_number(entry).unwrap_or(0);
        let path = self.decode.join(number.to_string());
        let scale = picture.scale_factor().max(1);
        let target = if shown_width > 0 && shown_height > 0 {
            (shown_width * scale, shown_height * scale)
        } else {
            (-1, -1)
        };
        let entry = entry.to_owned();
        glib::spawn_future_local(async move {
            let decoded = gio::spawn_blocking({
                let path = path.clone();
                move || decode_entry(&entry, &path)
            })
            .await;
            if decoded.is_err() {
                return;
            }
            if let Some(texture) = crate::ui::image::texture(path, target).await {
                picture.set_paintable(Some(&texture));
            }
        });
        frame.upcast()
    }
}

fn decode_entry(entry: &str, path: &std::path::Path) {
    if path.exists() {
        return;
    }
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let Ok(file) = std::fs::File::create(path) else {
        return;
    };
    let Ok(mut child) = std::process::Command::new("cliphist")
        .arg("decode")
        .stdin(std::process::Stdio::piped())
        .stdout(file)
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = writeln!(stdin, "{entry}");
    }
    let _ = child.wait();
}

fn app_icon(name: &str, size: i32, anchor: &impl IsA<gtk4::Widget>) -> gtk4::Widget {
    let image = gtk4::Image::new();
    image.set_pixel_size(size);
    image.set_size_request(size, size);
    image.set_valign(gtk4::Align::Center);
    if let Some(display) = gdk::Display::default() {
        let icons = gtk4::IconTheme::for_display(&display);
        let scale = anchor.scale_factor().max(1);
        image.set_paintable(Some(&crate::platform::appicon::themed(
            &icons,
            name,
            "image-missing",
            size,
            scale,
        )));
    }
    image.upcast()
}

fn placeholder(actions: &gtk4::Box) -> gtk4::Box {
    let (_, width, _, _) = actions.measure(gtk4::Orientation::Horizontal, -1);
    let (_, height, _, _) = actions.measure(gtk4::Orientation::Vertical, -1);
    let place = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    place.set_size_request(width, height);
    place
}

fn check_mark() -> gtk4::Widget {
    let symbol = text::symbol("check", pixel_size::NORMAL as f64);
    text::set_color(&symbol, "m3onPrimary");
    let holder = Centred::new(&symbol);
    let (_, height, _, _) = holder.measure(gtk4::Orientation::Vertical, -1);
    let circle = gtk4::Overlay::new();
    circle.add_css_class("search-check");
    circle.set_size_request(height, height);
    circle.set_valign(gtk4::Align::Center);
    circle.add_overlay(&holder);
    circle.upcast()
}

fn highlight(content: &str, query: &str, theme: &SharedTheme) -> String {
    if query.is_empty() || content == query {
        return glib::markup_escape_text(content).to_string();
    }
    let colour = theme.borrow().colors.col_primary;
    let hex = format!(
        "#{:02x}{:02x}{:02x}",
        (colour.red() * 255.0).round() as u8,
        (colour.green() * 255.0).round() as u8,
        (colour.blue() * 255.0).round() as u8
    );
    let wanted: Vec<char> = query.to_lowercase().chars().collect();
    let mut out = String::new();
    let mut next = 0;
    for letter in content.chars() {
        let escaped = glib::markup_escape_text(&letter.to_string()).to_string();
        let lower = letter.to_lowercase().next().unwrap_or(letter);
        if next < wanted.len() && lower == wanted[next] {
            out.push_str(&format!(
                "<u><span foreground=\"{hex}\">{escaped}</span></u>"
            ));
            next += 1;
        } else {
            out.push_str(&escaped);
        }
    }
    out
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
    let symbol_holder = Centred::new(&symbol);
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
            let colour = theme.borrow().colors.col_secondary_container;
            cr.set_source_rgba(
                colour.red() as f64,
                colour.green() as f64,
                colour.blue() as f64,
                colour.alpha() as f64,
            );
            shapes::polygon(shape.get()).trace(cr, 0.0, 0.0, size);
            let _ = cr.fill();
        }
    });
    let shaped = gtk4::Overlay::new();
    shaped.set_child(Some(&shape_area));
    shaped.add_overlay(&symbol_holder);
    shaped.set_valign(gtk4::Align::Center);

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
    bar.append(&shaped);
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
    crate::ui::widgets::flickable::follow_scroll_settings(&scroller);
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
        current: Cell::new(0),
        list_focused: Cell::new(false),
        dont_auto_cancel: Cell::new(false),
        animate_width: Cell::new(false),
        showing: Cell::new(false),
        width: Cell::new(Tween::new(
            SEARCH_WIDTH_COLLAPSED,
            WIDTH_MILLIS,
            EXPRESSIVE_DEFAULT,
        )),
        height: Cell::new(Tween::new(0.0, HEIGHT_MILLIS, EXPRESSIVE_DEFAULT)),
        ticking: Cell::new(false),
        results: RefCell::new(Vec::new()),
        highlight_query: RefCell::new(String::new()),
        clipboard: Rc::new(RefCell::new(String::new())),
        decode: glib::user_runtime_dir().join("proscenio/cliphist"),
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

    let _ = std::fs::remove_dir_all(&overview.decode);

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
    let near_end = {
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
    adjustment.connect_value_changed(near_end.clone());
    adjustment.connect_changed(near_end);

    scope.keep(launcher.subscribe({
        let overview = Rc::downgrade(&overview);
        move || {
            if let Some(overview) = overview.upgrade() {
                overview.results_changed();
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

    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    keys.connect_key_pressed({
        let overview = Rc::downgrade(&overview);
        move |_, key, _, state| {
            let Some(overview) = overview.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let control = state.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
            if key == gdk::Key::Escape {
                overview.hide();
                return glib::Propagation::Stop;
            }
            if !overview.list_focused.get() {
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
            let current = overview.current.get();
            match key {
                gdk::Key::Up if current == 0 => overview.focus_entry(),
                gdk::Key::Up => overview.select(current - 1),
                gdk::Key::Down => overview.select(current + 1),
                gdk::Key::Return | gdk::Key::KP_Enter => overview.activate(current),
                gdk::Key::Delete if shift => overview.delete_current(),
                gdk::Key::Tab => overview.complete(current),
                _ if control && key != gdk::Key::BackSpace => return glib::Propagation::Proceed,
                _ => {
                    if !overview.type_anywhere(key, control) {
                        return glib::Propagation::Proceed;
                    }
                }
            }
            glib::Propagation::Stop
        }
    });
    window.add_controller(keys);

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
