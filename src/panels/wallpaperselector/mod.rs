pub mod address;
pub mod tile;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::graphene;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use crate::core::config::{self, HYPRLAND_GAPS_OUT};
use crate::platform::{grab, hypr};
use crate::services::thumbnails::Size;
use crate::services::wallpapers::{Entry, Progress, Wallpapers, pictures};
use crate::ui::theme::{SharedTheme, pixel_size, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::progress::{self, ProgressBar};
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text::{self, Family};
use crate::ui::widgets::toolbar;
use crate::ui::widgets::tooltip::{self, Tooltip};
use address::AddressBar;
use tile::{State, Tile};

const NAMESPACE: &str = "proscenio:wallpaperSelector";
const WIDTH: i32 = 1200;
const HEIGHT: i32 = 690;
const ELEVATION: i32 = 10;
const INSET: i32 = 4;
const TITLE_MARGIN: i32 = 12;
const QUICK_WIDTH: i32 = 140;
const QUICK_HEIGHT: i32 = 38;
const QUICK_SPACING: i32 = 5;
const BUTTON_PADDING: (i32, i32) = (8, 6);
const COLUMNS: i32 = 4;
const ASPECT: f64 = 4.0 / 3.0;
const FIRST_ROWS: i32 = 5;
const MORE_ROWS: i32 = 4;
const COLUMN_SPACING: i32 = 5;
const PROGRESS_HEIGHT: i32 = 4;
const TOOLBAR_HEIGHT: i32 = 56;
const TOOLBAR_MARGIN: i32 = 8;
const TOOLBAR_SPACING: i32 = 6;
const TOOL_SIZE: i32 = 40;
const TOOL_ICON: f64 = 22.0;
const TOOL_ICON_DROP: i32 = 1;
const FILTER_WIDTH: i32 = 200;
const SWEEP_MICROS: f64 = 1_500_000.0;

const QUICK: Look = Look {
    toggled: |theme| theme.colors.col_secondary_container,
    toggled_hover: |theme| theme.colors.col_secondary_container_hover,
    ripple_toggled: |theme| theme.colors.col_secondary_container_active,
    ..LOOK_DEFAULT
};

const LOOK_DEFAULT: Look = Look {
    background: |theme| transparentize(theme.colors.col_layer1_hover, 1.0),
    hover: |theme| theme.colors.col_layer1_hover,
    toggled: |theme| theme.colors.col_primary,
    toggled_hover: |theme| theme.colors.col_primary_hover,
    ripple: |theme| theme.colors.col_layer1_active,
    ripple_toggled: |theme| theme.colors.col_primary_active,
};

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
    name: gtk4::Label,
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
    current: Cell<usize>,
    wallpaper: RefCell<PathBuf>,
    dark: Cell<bool>,
    filter: gtk4::Entry,
    address: Rc<AddressBar>,
    quick: Vec<QuickDir>,
    cell: (i32, i32),
    size: Size,
    bar: ProgressBar,
    sweep: Paint,
    mode_symbol: gtk4::Label,
    watched: RefCell<PathBuf>,
    watcher: RefCell<Option<gio::FileMonitor>>,
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
            self.wallpapers.open_fallback_picker(self.dark());
            return;
        }
        if self.view.borrow().is_some() {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn random(&self) {
        self.wallpapers.random(self.dark());
    }

    fn dark(&self) -> bool {
        self.theme.borrow().m3.darkmode
    }

    fn open(self: &Rc<Self>) {
        let display = gdk::Display::default();
        let wanted = hypr::focused_monitor();
        let monitor = display.as_ref().and_then(|display| {
            let monitors = display.monitors();
            let all: Vec<gdk::Monitor> = monitors.iter::<gdk::Monitor>().flatten().collect();
            all.iter()
                .find(|monitor| monitor.connector().map(|name| name.to_string()) == wanted)
                .or(all.first())
                .cloned()
        });
        let Some(monitor) = monitor else {
            return;
        };
        let view = View::build(self, &monitor);
        self.view.replace(Some(view.clone()));
        view.refresh(true);
        view.window.present();
        view.filter.grab_focus();
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
        view.wallpapers.stop_thumbnails();
        view.wallpapers.forget_listeners();
        view.window.destroy();
    }
}

impl View {
    fn build(selector: &Rc<WallpaperSelector>, monitor: &gdk::Monitor) -> Rc<Self> {
        let theme = &selector.theme;
        let wallpapers = &selector.wallpapers;

        let label = text::styled_sized("Pick a wallpaper", pixel_size::NORMAL);
        text::set_font(&label, Family::Main, pixel_size::NORMAL as f64, "wght=500");
        label.set_xalign(0.0);
        let title_text = Centred::filling_width(&label);
        let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        spacer.set_size_request(0, title_text.measure(gtk4::Orientation::Vertical, -1).1);
        let title = gtk4::Overlay::new();
        title.set_child(Some(&spacer));
        title.add_overlay(&title_text);
        title.set_margin_start(TITLE_MARGIN);
        title.set_margin_end(TITLE_MARGIN);
        title.set_margin_top(TITLE_MARGIN);
        title.set_margin_bottom(TITLE_MARGIN);

        let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        list.set_size_request(QUICK_WIDTH, -1);
        list.set_margin_start(INSET);
        list.set_margin_end(INSET);
        list.set_margin_top(INSET);
        list.set_margin_bottom(INSET);
        let mut quick = Vec::new();
        for (icon, name, path) in quick_dirs() {
            let symbol = text::symbol_filled(icon, pixel_size::LARGER as f64, 0.0);
            text::set_color(&symbol, "colOnLayer1");
            let label = text::styled(name);
            text::set_color(&label, "colOnLayer1");
            label.set_xalign(0.0);
            let name_holder = Centred::integral(&label);
            name_holder.set_hexpand(true);
            name_holder.set_halign(gtk4::Align::Start);
            let content_height = QUICK_HEIGHT - BUTTON_PADDING.1 * 2;
            let name_height = name_holder.measure(gtk4::Orientation::Vertical, -1).1;
            name_holder.set_valign(gtk4::Align::Start);
            name_holder
                .set_margin_top(((content_height - name_height) as f64 / 2.0).round() as i32);
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, QUICK_SPACING);
            if !icon.is_empty() {
                row.append(&Centred::integral(&symbol));
            }
            row.append(&name_holder);
            let button = RippleButton::new(theme);
            button.set_look(QUICK);
            button.set_radius(QUICK_HEIGHT as f64 / 2.0);
            button.set_content(&row, BUTTON_PADDING.0, BUTTON_PADDING.1);
            button.set_size_request(QUICK_WIDTH, QUICK_HEIGHT);
            button.set_sensitive(!icon.is_empty());
            let target = path.clone();
            button.connect_clicked({
                let wallpapers = wallpapers.clone();
                move |_| wallpapers.set_directory(&target.to_string_lossy())
            });
            list.append(&button);
            quick.push(QuickDir {
                button,
                path,
                symbol,
                name: label,
            });
        }

        let panel = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        panel.add_css_class("wallpaper-quick");
        panel.set_margin_start(INSET);
        panel.set_margin_top(INSET);
        panel.set_margin_bottom(INSET);
        panel.append(&title);
        panel.append(&list);
        panel.set_hexpand(false);
        let panel_width = panel.measure(gtk4::Orientation::Horizontal, -1).1;

        let address = AddressBar::new(theme, {
            let wallpapers = wallpapers.clone();
            move |path: &str| wallpapers.set_directory(path)
        });

        let grid_width = WIDTH - ELEVATION * 2 - panel_width;
        let cell_width = grid_width / COLUMNS;
        let cell_height = (cell_width as f64 / ASPECT).round() as i32;
        let inner = ((tile::MARGIN + tile::PADDING) * 2.0) as i32;
        let size = Size::for_dimensions((cell_width - inner) as f64, (cell_height - inner) as f64);

        let grid = gtk4::Grid::new();
        grid.set_valign(gtk4::Align::Start);
        grid.set_margin_bottom(TOOLBAR_HEIGHT);
        let scroller = gtk4::ScrolledWindow::new();
        scroller.add_css_class("wallpaper-grid");
        scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroller.set_overflow(gtk4::Overflow::Hidden);
        scroller.set_child(Some(&grid));
        crate::ui::widgets::flickable::follow_scroll_settings(&scroller);
        scroller.set_vexpand(true);
        scroller.set_size_request(grid_width, -1);

        let mut tips = Vec::new();
        let (open_button, _) = tool(theme, "open_in_new");
        tips.push(tip(
            theme,
            &open_button,
            "Use the system file picker instead\nRight-click to make this the default behavior",
        ));
        let (random_button, _) = tool(theme, "ifl");
        tips.push(tip(theme, &random_button, "Pick random from this folder"));
        let dark = theme.borrow().m3.darkmode;
        let (mode_button, mode_symbol) = tool(theme, if dark { "dark_mode" } else { "light_mode" });
        tips.push(tip(
            theme,
            &mode_button,
            "Click to toggle light/dark mode\n(applied when wallpaper is chosen)",
        ));

        let filter = gtk4::Entry::new();
        filter.add_css_class("wallpaper-filter");
        filter.set_has_frame(false);
        filter.set_size_request(FILTER_WIDTH, TOOL_SIZE);
        filter.set_valign(gtk4::Align::Center);
        filter.set_placeholder_text(Some("Hit \"/\" to search"));
        filter.set_attributes(&{
            let attributes = pango::AttrList::new();
            attributes.insert(pango::AttrFontDesc::new(&text::font(
                Family::Main,
                pixel_size::SMALL as f64,
                "wght=450",
            )));
            attributes
        });

        let bar = toolbar::frame();
        bar.append(&open_button);
        bar.append(&random_button);
        bar.append(&mode_button);
        bar.append(&filter);
        let (close_button, _, close_tip) =
            toolbar::paired_fab(theme, "close", "Cancel wallpaper selection");
        tips.push(close_tip);
        let options = gtk4::Box::new(gtk4::Orientation::Horizontal, TOOLBAR_SPACING);
        options.set_halign(gtk4::Align::Center);
        options.set_valign(gtk4::Align::End);
        options.set_margin_bottom(TOOLBAR_MARGIN);
        options.append(&bar);
        options.append(&close_button);

        let region = gtk4::Overlay::new();
        region.set_child(Some(&scroller));
        region.add_overlay(&options);
        region.set_vexpand(true);

        let progress_bar = ProgressBar::new();
        progress_bar.set_natural_width(0);
        progress_bar.set_hexpand(true);
        {
            let theme = theme.borrow();
            progress_bar.set_colours(progress::Colours {
                highlight: theme.colors.col_primary,
                track: theme.m3.secondary_container,
            });
        }
        let sweep = Paint::new(|_, _, _| {});
        sweep.set_hexpand(true);
        let gap = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        gap.set_size_request(-1, COLUMN_SPACING);
        gap.set_margin_start(INSET);
        gap.set_margin_end(INSET);
        let meters = gtk4::Stack::new();
        meters.set_valign(gtk4::Align::End);
        meters.set_size_request(-1, PROGRESS_HEIGHT);
        meters.add_named(&progress_bar, Some("part"));
        meters.add_named(&sweep, Some("sweep"));
        meters.add_named(
            &gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            Some("idle"),
        );
        meters.set_visible_child_name("idle");
        gap.append(&meters);

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        column.set_hexpand(true);
        column.append(&address.widget);
        column.append(&gap);
        column.append(&region);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.append(&panel);
        row.append(&column);

        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        card.add_css_class("wallpaper-selector");
        card.set_overflow(gtk4::Overflow::Hidden);
        card.set_size_request(WIDTH - ELEVATION * 2, HEIGHT - ELEVATION * 2);
        card.set_margin_start(ELEVATION);
        card.set_margin_end(ELEVATION);
        card.set_margin_top(ELEVATION);
        card.set_margin_bottom(ELEVATION);
        card.append(&row);

        let window = gtk4::ApplicationWindow::builder()
            .application(&selector.app)
            .child(&card)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        window.set_monitor(Some(monitor));
        window.set_layer(Layer::Overlay);
        window.set_anchor(Edge::Top, true);
        let current = config::current();
        window.set_margin(
            Edge::Top,
            if current.vertical {
                HYPRLAND_GAPS_OUT
            } else {
                current.bar_height() + HYPRLAND_GAPS_OUT
            },
        );
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::OnDemand);

        let view = Rc::new(View {
            selector: Rc::downgrade(selector),
            window: window.clone(),
            grab: grab::Grab::new(&monitor.display()),
            theme: theme.clone(),
            wallpapers: wallpapers.clone(),
            grid,
            scroller: scroller.clone(),
            tiles: RefCell::new(Vec::new()),
            entries: RefCell::new(Vec::new()),
            current: Cell::new(0),
            wallpaper: RefCell::new(PathBuf::new()),
            dark: Cell::new(dark),
            filter: filter.clone(),
            address,
            quick,
            cell: (cell_width, cell_height),
            size,
            bar: progress_bar,
            sweep: sweep.clone(),
            mode_symbol,
            watched: RefCell::new(PathBuf::new()),
            watcher: RefCell::new(None),
            _tips: tips,
        });

        sweep.set_draw({
            let view = Rc::downgrade(&view);
            move |snapshot, width, height| {
                let Some(view) = view.upgrade() else {
                    return;
                };
                let theme = view.theme.borrow();
                let track = transparentize(theme.colors.col_primary, 0.75);
                snapshot.append_color(&track, &graphene::Rect::new(0.0, 0.0, width, height));
                let now = view
                    .sweep
                    .frame_clock()
                    .map_or(0, |clock| clock.frame_time());
                let part = (now as f64 % SWEEP_MICROS) / SWEEP_MICROS;
                let length = width * 0.35;
                let start = (part as f32) * (width + length) - length;
                let left = start.max(0.0);
                let right = (start + length).min(width);
                if right > left {
                    snapshot.append_color(
                        &theme.colors.col_primary,
                        &graphene::Rect::new(left, 0.0, right - left, height),
                    );
                }
            }
        });
        sweep.add_tick_callback(|sweep, _| {
            sweep.queue_draw();
            glib::ControlFlow::Continue
        });

        wallpapers.on_changed({
            let view = Rc::downgrade(&view);
            move || {
                if let Some(view) = view.upgrade() {
                    view.refresh(false);
                }
            }
        });
        wallpapers.on_progress({
            let view = Rc::downgrade(&view);
            let meters = meters.clone();
            move || {
                let Some(view) = view.upgrade() else {
                    return;
                };
                match view.wallpapers.progress() {
                    Progress::Idle => meters.set_visible_child_name("idle"),
                    Progress::Starting => meters.set_visible_child_name("sweep"),
                    Progress::Part(part) => {
                        meters.set_visible_child_name("part");
                        view.bar.set_value(part);
                    }
                }
            }
        });
        wallpapers.on_thumbnail({
            let view = Rc::downgrade(&view);
            move |file| {
                let Some(view) = view.upgrade() else {
                    return;
                };
                for tile in view.tiles.borrow().iter() {
                    if tile.entry.path == file {
                        tile.load_thumbnail();
                    }
                }
            }
        });

        open_button.connect_clicked({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.wallpapers.open_fallback_picker(view.dark.get());
                    view.close();
                }
            }
        });
        open_button.connect_alt({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.wallpapers.open_fallback_picker(view.dark.get());
                    config::store_value(
                        "/wallpaperSelector/useSystemFileDialog",
                        Value::Bool(true),
                    );
                    view.close();
                }
            }
        });
        random_button.connect_clicked({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.wallpapers.random(view.dark.get());
                }
            }
        });
        mode_button.connect_clicked({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    let dark = !view.dark.get();
                    view.dark.set(dark);
                    view.mode_symbol
                        .set_text(if dark { "dark_mode" } else { "light_mode" });
                }
            }
        });
        close_button.connect_clicked({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.close();
                }
            }
        });
        filter.connect_changed({
            let view = Rc::downgrade(&view);
            move |filter| {
                if let Some(view) = view.upgrade() {
                    view.wallpapers.set_query(&filter.text());
                }
            }
        });
        let focus = gtk4::EventControllerFocus::new();
        let placeholder = |text: &'static str| {
            move |focus: &gtk4::EventControllerFocus| {
                if let Some(filter) = focus.widget().and_downcast::<gtk4::Entry>() {
                    filter.set_placeholder_text(Some(text));
                }
            }
        };
        focus.connect_enter(placeholder("Search wallpapers"));
        focus.connect_leave(placeholder("Hit \"/\" to search"));
        filter.add_controller(focus);

        scroller.vadjustment().connect_value_changed({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.grow_near_end();
                }
            }
        });

        let navigation = gtk4::GestureClick::new();
        navigation.set_button(0);
        navigation.connect_pressed({
            let view = Rc::downgrade(&view);
            move |gesture, _, _, _| {
                let Some(view) = view.upgrade() else {
                    return;
                };
                match gesture.current_button() {
                    8 => view.wallpapers.navigate_back(),
                    9 => view.wallpapers.navigate_forward(),
                    _ => {}
                }
            }
        });
        card.add_controller(navigation);

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let view = Rc::downgrade(&view);
            move |_, key, _, modifiers| {
                let Some(view) = view.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                view.key(key, modifiers)
            }
        });
        window.add_controller(keys);

        view
    }

    fn key(self: &Rc<Self>, key: gdk::Key, modifiers: gdk::ModifierType) -> glib::Propagation {
        let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        let alt = modifiers.contains(gdk::ModifierType::ALT_MASK);
        let typing = self.filter.has_focus();
        let editing = self.address.editing();
        if editing && key != gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match key {
            gdk::Key::Escape if editing => glib::Propagation::Proceed,
            gdk::Key::Escape => {
                self.close();
                glib::Propagation::Stop
            }
            gdk::Key::v | gdk::Key::V if control => self.paste(),
            gdk::Key::Up if alt => {
                self.wallpapers.navigate_up();
                glib::Propagation::Stop
            }
            gdk::Key::Left if alt => {
                self.wallpapers.navigate_back();
                glib::Propagation::Stop
            }
            gdk::Key::Right if alt => {
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
                self.activate(self.current.get());
                glib::Propagation::Stop
            }
            _ if typing => glib::Propagation::Proceed,
            gdk::Key::Left => {
                self.move_selection(-1);
                glib::Propagation::Stop
            }
            gdk::Key::Right => {
                self.move_selection(1);
                glib::Propagation::Stop
            }
            gdk::Key::BackSpace => {
                let text = self.filter.text();
                let mut chars = text.chars();
                chars.next_back();
                self.filter.set_text(chars.as_str());
                self.focus_filter();
                glib::Propagation::Stop
            }
            gdk::Key::l | gdk::Key::L if control => {
                self.address.focus_input();
                glib::Propagation::Stop
            }
            gdk::Key::slash => {
                self.focus_filter();
                glib::Propagation::Stop
            }
            _ => {
                if let Some(typed) = key.to_unicode().filter(|typed| !typed.is_control())
                    && !control
                    && !alt
                {
                    let mut text = self.filter.text().to_string();
                    text.push(typed);
                    self.filter.set_text(&text);
                    self.focus_filter();
                }
                glib::Propagation::Stop
            }
        }
    }

    fn focus_filter(&self) {
        self.filter.grab_focus();
        self.filter.set_position(-1);
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

    fn refresh(self: &Rc<Self>, moved: bool) {
        let directory = self.wallpapers.directory();
        let moved = moved || *self.watched.borrow() != directory;
        self.address.set_directory(&directory.to_string_lossy());
        for quick in &self.quick {
            let toggled = quick.path == directory;
            quick.button.set_toggled(toggled);
            let token = if toggled {
                "colOnSecondaryContainer"
            } else {
                "colOnLayer1"
            };
            text::set_color(&quick.symbol, token);
            text::set_color(&quick.name, token);
            text::set_symbol_font(
                &quick.symbol,
                pixel_size::LARGER as f64,
                if toggled { 1.0 } else { 0.0 },
            );
        }
        self.wallpaper.replace(PathBuf::from(
            config::value("/background/wallpaperPath")
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default(),
        ));

        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }
        self.tiles.borrow_mut().clear();
        self.entries.replace(self.wallpapers.entries());
        if moved {
            self.current.set(0);
            self.scroller.vadjustment().set_value(0.0);
            self.watch(&directory);
            self.wallpapers.generate_thumbnails(self.size);
        }
        let count = self.entries.borrow().len();
        self.current
            .set(self.current.get().min(count.saturating_sub(1)));
        self.scroller.set_visible(count > 0);
        self.grow((FIRST_ROWS * COLUMNS) as usize);
    }

    fn watch(self: &Rc<Self>, directory: &PathBuf) {
        self.watched.replace(directory.clone());
        let Ok(monitor) = gio::File::for_path(directory)
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        else {
            self.watcher.replace(None);
            return;
        };
        let pending = Rc::new(Cell::new(false));
        let view = Rc::downgrade(self);
        monitor.connect_changed(move |_, _, _, event| {
            if matches!(
                event,
                gio::FileMonitorEvent::Changed | gio::FileMonitorEvent::AttributeChanged
            ) || pending.replace(true)
            {
                return;
            }
            let view = view.clone();
            let pending = pending.clone();
            glib::idle_add_local_once(move || {
                pending.set(false);
                if let Some(view) = view.upgrade() {
                    view.refresh(false);
                }
            });
        });
        self.watcher.replace(Some(monitor));
    }

    fn grow(self: &Rc<Self>, wanted: usize) {
        let entries = self.entries.borrow();
        let built = self.tiles.borrow().len();
        let target = wanted.min(entries.len());
        if target <= built {
            return;
        }
        let wallpaper = self.wallpaper.borrow().clone();
        for index in built..target {
            let entry = entries[index].clone();
            let is_wallpaper = entry.path == wallpaper;
            let tile = Tile::new(&self.theme, entry, self.cell, self.size);
            tile.set_state(self.state_for(index, is_wallpaper), false);
            let motion = gtk4::EventControllerMotion::new();
            motion.connect_enter({
                let view = Rc::downgrade(self);
                move |_, _, _| {
                    if let Some(view) = view.upgrade() {
                        view.set_current(index);
                    }
                }
            });
            tile.widget.add_controller(motion);
            let click = gtk4::GestureClick::new();
            click.connect_released({
                let view = Rc::downgrade(self);
                move |_, _, _, _| {
                    if let Some(view) = view.upgrade() {
                        view.activate(index);
                    }
                }
            });
            tile.widget.add_controller(click);
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
        let reach = adjustment.value() + adjustment.page_size();
        if reach + (self.cell.1 * 2) as f64 >= adjustment.upper() {
            let built = self.tiles.borrow().len();
            self.grow(built + (MORE_ROWS * COLUMNS) as usize);
        }
    }

    fn state_for(&self, index: usize, is_wallpaper: bool) -> State {
        if index == self.current.get() {
            State::Current
        } else if is_wallpaper {
            State::Wallpaper
        } else {
            State::Plain
        }
    }

    fn set_current(self: &Rc<Self>, index: usize) {
        let previous = self.current.replace(index);
        let wallpaper = self.wallpaper.borrow().clone();
        let tiles = self.tiles.borrow();
        for changed in [previous, index] {
            if let Some(tile) = tiles.get(changed) {
                tile.set_state(self.state_for(changed, tile.entry.path == wallpaper), true);
            }
        }
    }

    fn move_selection(self: &Rc<Self>, delta: isize) {
        let count = self.entries.borrow().len();
        if count == 0 {
            return;
        }
        let index = (self.current.get() as isize + delta).clamp(0, count as isize - 1) as usize;
        self.grow(index + 1);
        self.set_current(index);
        let adjustment = self.scroller.vadjustment();
        let top = ((index as i32 / COLUMNS) * self.cell.1) as f64;
        let bottom = top + self.cell.1 as f64;
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
        self.filter.set_text("");
        self.wallpapers.select(&entry.path, self.dark.get());
    }
}

fn tool(theme: &SharedTheme, icon: &str) -> (RippleButton, gtk4::Label) {
    let symbol = text::symbol(icon, TOOL_ICON);
    text::set_color(&symbol, "colOnSurfaceVariant");
    let button = RippleButton::new(theme);
    button.set_look(QUICK);
    button.set_radius(TOOL_SIZE as f64 / 2.0);
    let holder = Centred::integral(&symbol);
    button.set_content(&holder, 0, 0);
    holder.set_margin_top(TOOL_ICON_DROP * 2);
    button.set_size_request(TOOL_SIZE, TOOL_SIZE);
    button.set_valign(gtk4::Align::Center);
    (button, symbol)
}

fn tip(theme: &SharedTheme, target: &RippleButton, text: &str) -> Rc<Tooltip> {
    let tip = Tooltip::new(target, theme, tooltip::Kind::Styled);
    tip.set_text(text);
    tooltip::hover_delay(target, &tip, 0);
    tip
}

fn quick_dirs() -> Vec<(&'static str, &'static str, PathBuf)> {
    let special = |folder| glib::user_special_dir(folder).unwrap_or_else(glib::home_dir);
    let mut dirs = vec![
        ("home", "Home", glib::home_dir()),
        ("docs", "Documents", special(glib::UserDirectory::Documents)),
        (
            "download",
            "Downloads",
            special(glib::UserDirectory::Downloads),
        ),
        ("image", "Pictures", pictures()),
        ("movie", "Videos", special(glib::UserDirectory::Videos)),
        ("", "---", PathBuf::from("INTENTIONALLY_INVALID_DIR")),
        ("wallpaper", "Wallpapers", pictures().join("Wallpapers")),
    ];
    if config::value("/policies/weeb").and_then(|value| value.as_i64()) == Some(1) {
        dirs.push(("favorite", "Homework", pictures().join("homework")));
    }
    dirs
}
