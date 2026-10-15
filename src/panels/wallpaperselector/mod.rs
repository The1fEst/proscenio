pub mod address;
mod builder;
pub mod tile;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use crate::core::config;
use crate::platform::hyprland::hypr;
use crate::platform::wayland::grab;
use crate::services::files::thumbnails::Size;
use crate::services::files::wallpapers::{Entry, Wallpapers};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::progress::ProgressBar;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::overlay::tooltip::Tooltip;
use address::AddressBar;
use tile::{State, Tile};

const COLUMNS: i32 = 4;
const FIRST_ROWS: i32 = 5;
const MORE_ROWS: i32 = 4;

pub struct WallpaperSelector {
    app: gtk4::Application,
    theme: SharedTheme,
    wallpapers: Rc<Wallpapers>,
    view: RefCell<Option<Rc<View>>>,
}

struct QuickDir {
    button: RippleButton,
    path: PathBuf,
    symbol: gtk4::Label,
    name_label: gtk4::Label,
}

struct View {
    selector: Weak<WallpaperSelector>,
    window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    theme: SharedTheme,
    wallpapers: Rc<Wallpapers>,
    grid: gtk4::Grid,
    scroller: gtk4::ScrolledWindow,
    tiles: RefCell<Vec<Rc<Tile>>>,
    entries: RefCell<Vec<Entry>>,
    current_index: Cell<usize>,
    wallpaper_path: RefCell<PathBuf>,
    applies_dark_mode: Cell<bool>,
    filter_field: gtk4::Entry,
    address_bar: Rc<AddressBar>,
    quick_dirs: Vec<QuickDir>,
    cell_size: (i32, i32),
    thumbnail_size: Size,
    progress_bar: ProgressBar,
    sweep: Paint,
    mode_symbol: gtk4::Label,
    watched_directory: RefCell<PathBuf>,
    watcher: RefCell<Option<gio::FileMonitor>>,
    listing_generation: Cell<u64>,
    is_listing_pending: Cell<bool>,
    is_activation_pending: Cell<bool>,
    is_reset_pending: Cell<bool>,
    _tips: Vec<Rc<Tooltip>>,
}

impl WallpaperSelector {
    pub fn new(
        app: &gtk4::Application,
        theme: &SharedTheme,
        wallpapers: &Rc<Wallpapers>,
    ) -> Rc<Self> {
        let selector = Rc::new(WallpaperSelector {
            app: app.clone(),
            theme: theme.clone(),
            wallpapers: wallpapers.clone(),
            view: RefCell::new(None),
        });
        wallpapers.on_applied({
            let selector = Rc::downgrade(&selector);
            move || {
                if let Some(selector) = selector.upgrade() {
                    selector.close();
                }
            }
        });
        selector
    }

    pub fn toggle(self: &Rc<Self>) {
        if config::value("/wallpaperSelector/useSystemFileDialog") == Some(Value::Bool(true)) {
            self.wallpapers.open_fallback_picker(self.is_dark_mode());
            return;
        }
        if self.view.borrow().is_some() {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn random(&self) {
        self.wallpapers.random(self.is_dark_mode());
    }

    fn is_dark_mode(&self) -> bool {
        self.theme.borrow().m3.is_dark_mode
    }

    fn open(self: &Rc<Self>) {
        let display = gdk::Display::default();
        let focused_connector = hypr::focused_monitor();
        let monitor = display.as_ref().and_then(|display| {
            let monitors = display.monitors();
            let all_monitors: Vec<gdk::Monitor> =
                monitors.iter::<gdk::Monitor>().flatten().collect();
            all_monitors
                .iter()
                .find(|monitor| {
                    monitor.connector().map(|name| name.to_string()) == focused_connector
                })
                .or(all_monitors.first())
                .cloned()
        });
        let Some(monitor) = monitor else {
            return;
        };
        let view = View::build(self, &monitor);
        self.view.replace(Some(view.clone()));
        view.refresh(true);
        view.window.present();
        view.filter_field.grab_focus();
        if let (Some(grab), Some(surface)) = (view.grab.as_ref(), view.window.surface()) {
            let selector = Rc::downgrade(self);
            grab.hold(&surface, move || {
                if let Some(selector) = selector.upgrade() {
                    selector.close();
                }
            });
        }
    }

    pub fn close(&self) {
        let Some(view) = self.view.take() else {
            return;
        };
        if let Some(grab) = view.grab.as_ref() {
            grab.release();
        }
        view.listing_generation
            .set(view.listing_generation.get() + 1);
        view.wallpapers.stop_thumbnails();
        view.wallpapers.forget_listeners();
        view.window.destroy();
    }
}

impl View {
    fn handle_key(
        self: &Rc<Self>,
        key: gdk::Key,
        modifiers: gdk::ModifierType,
    ) -> glib::Propagation {
        let is_control_held = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        let is_alt_held = modifiers.contains(gdk::ModifierType::ALT_MASK);
        let is_typing = self.filter_field.has_focus();
        let is_editing = self.address_bar.is_editing();
        if is_editing && key != gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match key {
            gdk::Key::Escape if is_editing => glib::Propagation::Proceed,
            gdk::Key::Escape => {
                self.close();
                glib::Propagation::Stop
            }
            gdk::Key::v | gdk::Key::V if is_control_held => self.paste(),
            gdk::Key::Up if is_alt_held => {
                self.wallpapers.navigate_up();
                glib::Propagation::Stop
            }
            gdk::Key::Left if is_alt_held => {
                self.wallpapers.navigate_back();
                glib::Propagation::Stop
            }
            gdk::Key::Right if is_alt_held => {
                self.wallpapers.navigate_forward();
                glib::Propagation::Stop
            }
            gdk::Key::Up => {
                self.move_selection(-COLUMNS as isize);
                glib::Propagation::Stop
            }
            gdk::Key::Down => {
                self.move_selection(COLUMNS as isize);
                glib::Propagation::Stop
            }
            gdk::Key::Return | gdk::Key::KP_Enter => {
                if self.is_listing_pending.get() {
                    self.is_activation_pending.set(true);
                } else {
                    self.activate(self.current_index.get());
                }
                glib::Propagation::Stop
            }
            _ if is_typing => glib::Propagation::Proceed,
            gdk::Key::Left => {
                self.move_selection(-1);
                glib::Propagation::Stop
            }
            gdk::Key::Right => {
                self.move_selection(1);
                glib::Propagation::Stop
            }
            gdk::Key::BackSpace => {
                let text = self.filter_field.text();
                let mut chars = text.chars();
                chars.next_back();
                self.filter_field.set_text(chars.as_str());
                self.focus_filter();
                glib::Propagation::Stop
            }
            gdk::Key::l | gdk::Key::L if is_control_held => {
                self.address_bar.focus_input();
                glib::Propagation::Stop
            }
            gdk::Key::slash => {
                self.focus_filter();
                glib::Propagation::Stop
            }
            _ => {
                if let Some(character) =
                    key.to_unicode().filter(|character| !character.is_control())
                    && !is_control_held
                    && !is_alt_held
                {
                    let mut text = self.filter_field.text().to_string();
                    text.push(character);
                    self.filter_field.set_text(&text);
                    self.focus_filter();
                }
                glib::Propagation::Stop
            }
        }
    }

    fn focus_filter(&self) {
        self.filter_field.grab_focus();
        self.filter_field.set_position(-1);
    }

    fn paste(self: &Rc<Self>) -> glib::Propagation {
        let clipboard = self.window.clipboard();
        let formats = clipboard.formats();
        if !formats.contains_type(gdk::FileList::static_type())
            && !formats.contain_mime_type("text/uri-list")
        {
            return glib::Propagation::Proceed;
        }
        let wallpapers = self.wallpapers.clone();
        glib::spawn_future_local(async move {
            let Ok(value) = clipboard
                .read_value_future(gdk::FileList::static_type(), glib::Priority::DEFAULT)
                .await
            else {
                return;
            };
            let Ok(files) = value.get::<gdk::FileList>() else {
                return;
            };
            if let Some(path) = files.files().first().and_then(|file| file.path()) {
                wallpapers.set_directory(&path.to_string_lossy());
            }
        });
        glib::Propagation::Stop
    }

    fn close(&self) {
        if let Some(selector) = self.selector.upgrade() {
            selector.close();
        }
    }

    fn refresh(self: &Rc<Self>, should_reset: bool) {
        let directory = self.wallpapers.directory();
        let should_reset = should_reset || *self.watched_directory.borrow() != directory;
        self.address_bar.set_directory(&directory.to_string_lossy());
        for quick_dir in &self.quick_dirs {
            let is_toggled = quick_dir.path == directory;
            quick_dir.button.set_toggled(is_toggled);
            let text_token = if is_toggled {
                "colOnSecondaryContainer"
            } else {
                "colOnLayer1"
            };
            text::set_color(&quick_dir.symbol, text_token);
            text::set_color(&quick_dir.name_label, text_token);
            text::set_symbol_font(
                &quick_dir.symbol,
                pixel_size::LARGER as f64,
                if is_toggled { 1.0 } else { 0.0 },
            );
        }
        self.wallpaper_path.replace(PathBuf::from(
            config::value("/background/wallpaperPath")
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default(),
        ));

        self.is_reset_pending
            .set(self.is_reset_pending.get() || should_reset);
        let generation = self.listing_generation.get() + 1;
        self.listing_generation.set(generation);
        self.is_listing_pending.set(true);
        let view = Rc::downgrade(self);
        let wallpapers = self.wallpapers.clone();
        glib::spawn_future_local(async move {
            let entries = wallpapers.entries().await;
            if let Some(view) = view.upgrade()
                && view.listing_generation.get() == generation
            {
                view.is_listing_pending.set(false);
                view.show_entries(entries, &directory);
            }
        });
    }

    fn show_entries(self: &Rc<Self>, entries: Vec<Entry>, directory: &Path) {
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }
        self.tiles.borrow_mut().clear();
        self.entries.replace(entries);
        if self.is_reset_pending.replace(false) {
            self.current_index.set(0);
            self.scroller.vadjustment().set_value(0.0);
            self.watch(directory);
            self.wallpapers.generate_thumbnails(self.thumbnail_size);
        }
        let count = self.entries.borrow().len();
        self.current_index
            .set(self.current_index.get().min(count.saturating_sub(1)));
        self.scroller.set_visible(count > 0);
        self.grow((FIRST_ROWS * COLUMNS) as usize);
        if self.is_activation_pending.replace(false) {
            self.activate(self.current_index.get());
        }
    }

    fn watch(self: &Rc<Self>, directory: &Path) {
        self.watched_directory.replace(directory.to_path_buf());
        let Ok(monitor) = gio::File::for_path(directory)
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        else {
            self.watcher.replace(None);
            return;
        };
        let is_pending = Rc::new(Cell::new(false));
        let view = Rc::downgrade(self);
        monitor.connect_changed(move |_, file, other_file, event| {
            let written_file = match event {
                gio::FileMonitorEvent::ChangesDoneHint | gio::FileMonitorEvent::MovedIn => {
                    Some(file)
                }
                gio::FileMonitorEvent::Renamed => other_file,
                _ => None,
            };
            if let Some(path) = written_file.and_then(|file| file.path())
                && let Some(view) = view.upgrade()
            {
                view.wallpapers
                    .generate_thumbnail(&path, view.thumbnail_size);
            }
            if matches!(
                event,
                gio::FileMonitorEvent::Changed | gio::FileMonitorEvent::AttributeChanged
            ) || is_pending.replace(true)
            {
                return;
            }
            let view = view.clone();
            let is_pending = is_pending.clone();
            glib::idle_add_local_once(move || {
                is_pending.set(false);
                if let Some(view) = view.upgrade() {
                    view.refresh(false);
                }
            });
        });
        self.watcher.replace(Some(monitor));
    }

    fn grow(self: &Rc<Self>, requested_count: usize) {
        let entries = self.entries.borrow();
        let existing_count = self.tiles.borrow().len();
        let target_count = requested_count.min(entries.len());
        if target_count <= existing_count {
            return;
        }
        let wallpaper_path = self.wallpaper_path.borrow().clone();
        for index in existing_count..target_count {
            let entry = entries[index].clone();
            let is_wallpaper = entry.path == wallpaper_path;
            let tile = Tile::new(&self.theme, entry, self.cell_size, self.thumbnail_size);
            tile.set_state(self.state_for(index, is_wallpaper), false);
            let motion_controller = gtk4::EventControllerMotion::new();
            motion_controller.connect_enter({
                let view = Rc::downgrade(self);
                move |_, _, _| {
                    if let Some(view) = view.upgrade() {
                        view.set_current_index(index);
                    }
                }
            });
            tile.widget.add_controller(motion_controller);
            let click_gesture = gtk4::GestureClick::new();
            click_gesture.connect_released({
                let view = Rc::downgrade(self);
                move |_, _, _, _| {
                    if let Some(view) = view.upgrade() {
                        view.activate(index);
                    }
                }
            });
            tile.widget.add_controller(click_gesture);
            self.grid.attach(
                &tile.widget,
                (index as i32) % COLUMNS,
                (index as i32) / COLUMNS,
                1,
                1,
            );
            tile.load_thumbnail();
            self.tiles.borrow_mut().push(tile);
        }
    }

    fn grow_near_end(self: &Rc<Self>) {
        let adjustment = self.scroller.vadjustment();
        let visible_bottom = adjustment.value() + adjustment.page_size();
        if visible_bottom + (self.cell_size.1 * 2) as f64 >= adjustment.upper() {
            let existing_count = self.tiles.borrow().len();
            self.grow(existing_count + (MORE_ROWS * COLUMNS) as usize);
        }
    }

    fn state_for(&self, index: usize, is_wallpaper: bool) -> State {
        if index == self.current_index.get() {
            State::Current
        } else if is_wallpaper {
            State::Wallpaper
        } else {
            State::Plain
        }
    }

    fn set_current_index(self: &Rc<Self>, index: usize) {
        let previous_index = self.current_index.replace(index);
        let wallpaper_path = self.wallpaper_path.borrow().clone();
        let tiles = self.tiles.borrow();
        for tile_index in [previous_index, index] {
            if let Some(tile) = tiles.get(tile_index) {
                tile.set_state(
                    self.state_for(tile_index, tile.entry.path == wallpaper_path),
                    true,
                );
            }
        }
    }

    fn move_selection(self: &Rc<Self>, delta: isize) {
        let count = self.entries.borrow().len();
        if count == 0 {
            return;
        }
        let index =
            (self.current_index.get() as isize + delta).clamp(0, count as isize - 1) as usize;
        self.grow(index + 1);
        self.set_current_index(index);
        let adjustment = self.scroller.vadjustment();
        let top = ((index as i32 / COLUMNS) * self.cell_size.1) as f64;
        let bottom = top + self.cell_size.1 as f64;
        if top < adjustment.value() {
            adjustment.set_value(top);
        } else if bottom > adjustment.value() + adjustment.page_size() {
            adjustment.set_value(bottom - adjustment.page_size());
        }
    }

    fn activate(self: &Rc<Self>, index: usize) {
        let Some(entry) = self.entries.borrow().get(index).cloned() else {
            return;
        };
        self.filter_field.set_text("");
        self.wallpapers
            .select(&entry.path, self.applies_dark_mode.get());
    }
}
