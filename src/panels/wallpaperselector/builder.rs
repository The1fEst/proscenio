use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use super::address::AddressBar;
use super::{COLUMNS, QuickDir, View, WallpaperSelector, tile};
use crate::core::config::{self, HYPRLAND_GAPS_OUT};
use crate::core::i18n::tr;
use crate::platform::wayland::grab;
use crate::services::files::thumbnails::Size;
use crate::services::files::wallpapers::{Progress, pictures};
use crate::ui::theme::{SharedTheme, pixel_size, transparentize};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::progress::{self, ProgressBar};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::toolbar;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

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
const ASPECT: f64 = 4.0 / 3.0;
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

impl View {
    pub(super) fn build(selector: &Rc<WallpaperSelector>, monitor: &gdk::Monitor) -> Rc<Self> {
        let theme = &selector.theme;
        let wallpapers = &selector.wallpapers;

        let title_label = text::styled_sized(&tr("Pick a wallpaper"), pixel_size::NORMAL);
        text::set_font(
            &title_label,
            Family::Main,
            pixel_size::NORMAL as f64,
            "wght=500",
        );
        title_label.set_xalign(0.0);
        let title_box = Centered::filling_width(&title_label);
        let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        spacer.set_size_request(0, title_box.measure(gtk4::Orientation::Vertical, -1).1);
        let title = gtk4::Overlay::new();
        title.set_child(Some(&spacer));
        title.add_overlay(&title_box);
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
        let mut quick_dir_items = Vec::new();
        for (icon, name, path) in quick_dirs() {
            let symbol = text::symbol_filled(icon, pixel_size::LARGER as f64, 0.0);
            text::set_color(&symbol, "colOnLayer1");
            let name_label = text::styled(&tr(name));
            text::set_color(&name_label, "colOnLayer1");
            name_label.set_xalign(0.0);
            let name_box = Centered::integral(&name_label);
            name_box.set_hexpand(true);
            name_box.set_halign(gtk4::Align::Start);
            let content_height = QUICK_HEIGHT - BUTTON_PADDING.1 * 2;
            let name_height = name_box.measure(gtk4::Orientation::Vertical, -1).1;
            name_box.set_valign(gtk4::Align::Start);
            name_box.set_margin_top(((content_height - name_height) as f64 / 2.0).round() as i32);
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, QUICK_SPACING);
            if !icon.is_empty() {
                row.append(&Centered::integral(&symbol));
            }
            row.append(&name_box);
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
            quick_dir_items.push(QuickDir {
                button,
                path,
                symbol,
                name_label,
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

        let address_bar = AddressBar::new(theme, {
            let wallpapers = wallpapers.clone();
            move |path: &str| wallpapers.set_directory(path)
        });

        let grid_width = WIDTH - ELEVATION * 2 - panel_width;
        let cell_width = grid_width / COLUMNS;
        let cell_height = (cell_width as f64 / ASPECT).round() as i32;
        let total_inset = ((tile::MARGIN + tile::PADDING) * 2.0) as i32;
        let thumbnail_size = Size::for_dimensions(
            (cell_width - total_inset) as f64,
            (cell_height - total_inset) as f64,
        );

        let grid = gtk4::Grid::new();
        grid.set_valign(gtk4::Align::Start);
        grid.set_margin_bottom(TOOLBAR_HEIGHT);
        let scroller = gtk4::ScrolledWindow::new();
        scroller.add_css_class("wallpaper-grid");
        scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroller.set_overflow(gtk4::Overflow::Hidden);
        scroller.set_child(Some(&grid));
        crate::ui::widgets::input::flickable::follow_scroll_settings(&scroller);
        scroller.set_vexpand(true);
        scroller.set_size_request(grid_width, -1);

        let mut tips = Vec::new();
        let (open_button, _) = tool(theme, "open_in_new");
        tips.push(tip(
            theme,
            &open_button,
            &tr(
                "Use the system file picker instead\nRight-click to make this the default behavior",
            ),
        ));
        let (random_button, _) = tool(theme, "ifl");
        tips.push(tip(
            theme,
            &random_button,
            &tr("Pick random from this folder"),
        ));
        let is_dark_mode = theme.borrow().m3.is_dark_mode;
        let (mode_button, mode_symbol) = tool(
            theme,
            if is_dark_mode {
                "dark_mode"
            } else {
                "light_mode"
            },
        );
        tips.push(tip(
            theme,
            &mode_button,
            &tr("Click to toggle light/dark mode\n(applied when wallpaper is chosen)"),
        ));

        let filter_field = gtk4::Entry::new();
        filter_field.add_css_class("wallpaper-filter");
        filter_field.set_has_frame(false);
        filter_field.set_size_request(FILTER_WIDTH, TOOL_SIZE);
        filter_field.set_valign(gtk4::Align::Center);
        filter_field.set_placeholder_text(Some(&tr("Hit \"/\" to search")));
        filter_field.set_attributes(&{
            let attributes = pango::AttrList::new();
            attributes.insert(pango::AttrFontDesc::new(&text::font(
                Family::Main,
                pixel_size::SMALL as f64,
                "wght=450",
            )));
            attributes
        });

        let toolbar_frame = toolbar::frame();
        toolbar_frame.append(&open_button);
        toolbar_frame.append(&random_button);
        toolbar_frame.append(&mode_button);
        toolbar_frame.append(&filter_field);
        let (close_button, _, close_tip) =
            toolbar::paired_fab(theme, "close", &tr("Cancel wallpaper selection"));
        tips.push(close_tip);
        let options = gtk4::Box::new(gtk4::Orientation::Horizontal, TOOLBAR_SPACING);
        options.set_halign(gtk4::Align::Center);
        options.set_valign(gtk4::Align::End);
        options.set_margin_bottom(TOOLBAR_MARGIN);
        options.append(&toolbar_frame);
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
            progress_bar.set_colors(progress::Colors {
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
        column.append(&address_bar.widget);
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
        let current_config = config::current();
        window.set_margin(
            Edge::Top,
            if current_config.vertical {
                HYPRLAND_GAPS_OUT
            } else {
                current_config.bar_height() + HYPRLAND_GAPS_OUT
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
            current_index: Cell::new(0),
            wallpaper_path: RefCell::new(PathBuf::new()),
            applies_dark_mode: Cell::new(is_dark_mode),
            filter_field: filter_field.clone(),
            address_bar,
            quick_dirs: quick_dir_items,
            cell_size: (cell_width, cell_height),
            thumbnail_size,
            progress_bar,
            sweep: sweep.clone(),
            mode_symbol,
            watched_directory: RefCell::new(PathBuf::new()),
            watcher: RefCell::new(None),
            listing_generation: Cell::new(0),
            is_listing_pending: Cell::new(false),
            is_activation_pending: Cell::new(false),
            is_reset_pending: Cell::new(false),
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
                        view.progress_bar.set_value(part);
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
                    view.wallpapers
                        .open_fallback_picker(view.applies_dark_mode.get());
                    view.close();
                }
            }
        });
        open_button.connect_alt({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.wallpapers
                        .open_fallback_picker(view.applies_dark_mode.get());
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
                    view.wallpapers.random(view.applies_dark_mode.get());
                }
            }
        });
        mode_button.connect_clicked({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    let applies_dark_mode = !view.applies_dark_mode.get();
                    view.applies_dark_mode.set(applies_dark_mode);
                    view.mode_symbol.set_text(if applies_dark_mode {
                        "dark_mode"
                    } else {
                        "light_mode"
                    });
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
        filter_field.connect_changed({
            let view = Rc::downgrade(&view);
            move |filter_field| {
                if let Some(view) = view.upgrade() {
                    view.wallpapers.set_query(&filter_field.text());
                }
            }
        });
        let focus_controller = gtk4::EventControllerFocus::new();
        let placeholder_handler = |text: &'static str| {
            move |focus_controller: &gtk4::EventControllerFocus| {
                if let Some(filter_field) = focus_controller.widget().and_downcast::<gtk4::Entry>()
                {
                    filter_field.set_placeholder_text(Some(&tr(text)));
                }
            }
        };
        focus_controller.connect_enter(placeholder_handler("Search wallpapers"));
        focus_controller.connect_leave(placeholder_handler("Hit \"/\" to search"));
        filter_field.add_controller(focus_controller);

        scroller.vadjustment().connect_value_changed({
            let view = Rc::downgrade(&view);
            move |_| {
                if let Some(view) = view.upgrade() {
                    view.grow_near_end();
                }
            }
        });

        let navigation_gesture = gtk4::GestureClick::new();
        navigation_gesture.set_button(0);
        navigation_gesture.connect_pressed({
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
        card.add_controller(navigation_gesture);

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let view = Rc::downgrade(&view);
            move |_, key, _, modifiers| {
                let Some(view) = view.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                view.handle_key(key, modifiers)
            }
        });
        window.add_controller(key_controller);

        view
    }
}

fn tool(theme: &SharedTheme, icon: &str) -> (RippleButton, gtk4::Label) {
    let symbol = text::symbol(icon, TOOL_ICON);
    text::set_color(&symbol, "colOnSurfaceVariant");
    let button = RippleButton::new(theme);
    button.set_look(QUICK);
    button.set_radius(TOOL_SIZE as f64 / 2.0);
    let symbol_box = Centered::integral(&symbol);
    button.set_content(&symbol_box, 0, 0);
    symbol_box.set_margin_top(TOOL_ICON_DROP * 2);
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
    let special_dir = |folder| glib::user_special_dir(folder).unwrap_or_else(glib::home_dir);
    let mut dirs = vec![
        ("home", "Home", glib::home_dir()),
        (
            "docs",
            "Documents",
            special_dir(glib::UserDirectory::Documents),
        ),
        (
            "download",
            "Downloads",
            special_dir(glib::UserDirectory::Downloads),
        ),
        ("image", "Pictures", pictures()),
        ("movie", "Videos", special_dir(glib::UserDirectory::Videos)),
        ("", "---", PathBuf::from("INTENTIONALLY_INVALID_DIR")),
        ("wallpaper", "Wallpapers", pictures().join("Wallpapers")),
    ];
    if config::value("/policies/weeb").and_then(|value| value.as_i64()) == Some(1) {
        dirs.push(("favorite", "Homework", pictures().join("homework")));
    }
    dirs
}
