mod interaction;
mod options;
mod renderer;
pub mod screenshot;
mod targets;

use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::config::{self, Config};
use crate::core::i18n::tr;
use crate::core::process;
use crate::platform::hyprland::hypr;
use crate::services::hyprland::cursorhider::CursorHider;
use crate::services::media::recording::Recording;
use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::ConfigSwitch;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::toolbar::{self, TabBar};
use crate::ui::widgets::overlay::tooltip::Tooltip;
use interaction::{guide_description, guide_symbol};
use options::options_menu;
use screenshot::Action;
use targets::{Target, targets};

const NAMESPACE: &str = "proscenio:regionSelector";
const OVERLAY_ALPHA: f32 = 0.6;
const DASH: f64 = 8.0;
const GAP: f64 = 4.0;
const LABEL_MARGIN: f64 = 8.0;
const AIM_OPACITY: f32 = 0.2;
const REGION_RADIUS: f64 = rounding::WINDOW_ROUNDING as f64;
const REGION_BORDER: f32 = 2.0;
const REGION_BORDER_TARGETED: f32 = 4.0;
const REGION_TEXT_PADDING: f64 = 10.0;
const TAG_PADDING: (f64, f64) = (10.0, 5.0);
const TAG_RADIUS: f64 = 10.0;
const TAG_SPACING: f64 = 4.0;
const FAST_MILLIS: f64 = 200.0;
const MOVE_MILLIS: f64 = 500.0;
const BREATH_MILLIS: f64 = 1200.0;
const BREATH_HIGH: f64 = 0.9;
const BREATH_LOW: f64 = 0.3;
const GUIDE_MARGIN: i32 = 8;
const GUIDE_HEIGHT: i32 = 38;
const GUIDE_PADDING: i32 = 8;
const GUIDE_CORNER: f64 = 6.0;
const GUIDE_SPACING: i32 = 12;
const GUIDE_TEXT_INSET: i32 = 6;
const GUIDE_ICON: f64 = 22.0;
const DESCRIPTION_TIMEOUT: Duration = Duration::from_secs(1);
const TOOLBAR_ICON: f64 = 22.0;
const ARROW_ICON: f64 = 20.0;
const OPTIONS_SPACING: i32 = 6;
const CONTROLS_SPACING: i32 = 6;
const CONTROLS_MARGIN: i32 = 8;
const MENU_WIDTH: i32 = 340;
const MENU_SPACING: i32 = 2;
const LABEL_INSET: i32 = 2;
const CHOICE_PADDING: (i32, i32) = (12, 8);
const CHOICE_SQUARE: f64 = rounding::UNSHARPENMORE as f64;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Screen,
    Window,
    Region,
    Shape,
    RecordScreen,
    RecordRegion,
}

impl Mode {
    fn is_whole_screen(self) -> bool {
        matches!(self, Mode::Screen | Mode::RecordScreen)
    }

    fn is_recording(self) -> bool {
        matches!(self, Mode::RecordScreen | Mode::RecordRegion)
    }

    fn is_circle(self) -> bool {
        self == Mode::Shape
    }

    fn picks_region(self) -> bool {
        matches!(self, Mode::Window | Mode::Region)
    }
}

const MODES: [(Mode, &str, &str); 6] = [
    (Mode::Screen, "fullscreen", "Whole screen"),
    (
        Mode::Window,
        "select_window",
        "With rounded corners and a shadow",
    ),
    (Mode::Region, "activity_zone", "As it is on screen"),
    (Mode::Shape, "gesture", "A drawn shape"),
    (Mode::RecordScreen, "screen_record", "Record the screen"),
    (Mode::RecordRegion, "screenshot_region", "Record a region"),
];

const COUNTDOWNS: [(i64, &str); 3] = [(0, "None"), (5, "5s"), (10, "10s")];

fn mode_index(mode: Mode) -> usize {
    MODES
        .iter()
        .position(|(candidate, _, _)| *candidate == mode)
        .unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Select,
    Post,
}

#[derive(Default)]
struct Drag {
    start: (f64, f64),
    end: (f64, f64),
    delta: (f64, f64),
    is_dragging: bool,
    points: Vec<(f64, f64)>,
}

struct Guide {
    widget: gtk4::Overlay,
    background: Paint,
    set_symbol: Rc<dyn Fn(&str)>,
    description_label: gtk4::Label,
    width_tween: Cell<Tween>,
    fade: Cell<Tween>,
    hide_timer: RefCell<Option<glib::SourceId>>,
}

struct Controls {
    holder: text::Shift,
    tabs: Rc<TabBar>,
    capture_button: RippleButton,
    capture_symbol: gtk4::Label,
    capture_tip: Rc<Tooltip>,
    options_button: RippleButton,
    arrow_symbol: gtk4::Label,
    menu: gtk4::Box,
    is_open: Cell<bool>,
    rise_tween: Cell<Tween>,
    appear_tween: Cell<Tween>,
    _switches: Vec<Rc<ConfigSwitch>>,
    _update_countdown: Rc<dyn Fn(bool)>,
}

struct Selection {
    selector: Weak<RegionSelector>,
    controls: Controls,
    config: Rc<Config>,
    theme: SharedTheme,
    window: gtk4::ApplicationWindow,
    paint: Paint,
    layer: gtk4::Fixed,
    guide: Guide,
    screen: String,
    size: (f64, f64),
    scale: f64,
    offset: (f64, f64),
    path: PathBuf,
    mode: Cell<Mode>,
    phase: Cell<Phase>,
    screenshot_texture: RefCell<Option<gdk::Texture>>,
    drag: RefCell<Drag>,
    region: Cell<[f64; 4]>,
    target_bounds: Cell<Option<[f64; 4]>>,
    windows: Vec<Target>,
    layers: Vec<Target>,
    fills: RefCell<Vec<Tween>>,
    fade: Cell<Tween>,
    button: Cell<u32>,
    pointer: Cell<(f64, f64)>,
    action: Cell<Action>,
    breath_start: Cell<i64>,
    is_ticking: Cell<bool>,
    has_snipped: Cell<bool>,
}

pub struct RegionSelector {
    app: gtk4::Application,
    theme: SharedTheme,
    recording: Recording,
    cursor_hider: CursorHider,
    mode: Cell<Mode>,
    recording_screen: RefCell<String>,
    selections: RefCell<Vec<Rc<Selection>>>,
}

impl RegionSelector {
    pub fn new(
        app: &gtk4::Application,
        theme: &SharedTheme,
        recording: &Recording,
        cursor_hider: &CursorHider,
    ) -> Rc<Self> {
        let selector = Rc::new(RegionSelector {
            app: app.clone(),
            theme: theme.clone(),
            recording: recording.clone(),
            cursor_hider: cursor_hider.clone(),
            mode: Cell::new(Mode::Region),
            recording_screen: RefCell::new(String::new()),
            selections: RefCell::new(Vec::new()),
        });
        recording
            .subscribe({
                let selector = Rc::downgrade(&selector);
                move || {
                    let Some(selector) = selector.upgrade() else {
                        return;
                    };
                    if !selector.recording.is_active()
                        && !selector.recording_screen.borrow().is_empty()
                    {
                        selector.dismiss();
                    }
                }
            })
            .detach();
        selector
    }

    fn config(&self) -> Rc<Config> {
        config::current()
    }

    pub fn screenshot(self: &Rc<Self>) {
        self.mode.set(Mode::Region);
        self.reopen();
    }

    pub fn record(self: &Rc<Self>) {
        self.config().region.set_record_sound(false);
        self.mode.set(Mode::RecordRegion);
        self.reopen();
    }

    pub fn record_with_sound(self: &Rc<Self>) {
        self.config().region.set_record_sound(true);
        self.mode.set(Mode::RecordRegion);
        self.reopen();
    }

    pub fn stop_recording(&self) {
        self.recording.stop();
    }

    fn reopen(self: &Rc<Self>) {
        self.recording_screen.replace(String::new());
        self.close_all();
        if self.mode.get().is_recording() && process::is_running("wf-recorder") {
            process::launch_subcommand(&["record"]);
            return;
        }
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let monitors: Vec<gdk::Monitor> = display
            .monitors()
            .iter::<gdk::Monitor>()
            .flatten()
            .collect();
        for monitor in monitors {
            let selection = self.build(&monitor);
            self.selections.borrow_mut().push(selection.clone());
            self.prepare(&selection);
        }
    }

    pub fn dismiss(&self) {
        self.recording_screen.replace(String::new());
        self.close_all();
    }

    fn close_all(&self) {
        let selections: Vec<Rc<Selection>> = self.selections.borrow_mut().drain(..).collect();
        for selection in selections {
            close(&selection);
        }
    }

    fn handle_recording_start(&self, screen: &str) {
        self.recording_screen.replace(screen.to_owned());
        let other_selections: Vec<Rc<Selection>> = {
            let mut selections = self.selections.borrow_mut();
            let (recording_selections, other_selections) = selections
                .drain(..)
                .partition(|selection| selection.screen == screen);
            *selections = recording_selections;
            other_selections
        };
        for selection in other_selections {
            close(&selection);
        }
        self.recording.watch();
    }

    fn prepare(&self, selection: &Rc<Selection>) {
        let shows_pointer = selection.config.region.show_pointer.get();
        let cursor_guard = shows_pointer.then(|| self.cursor_hider.keep_cursor_visible());
        if shows_pointer {
            hypr::show_cursor();
        }
        let screenshot_command =
            screenshot::temp_command(&selection.screen, &selection.path, shows_pointer);
        let selection = Rc::downgrade(selection);
        glib::spawn_future_local(async move {
            let _ = run(&["bash", "-c", &screenshot_command]).await;
            drop(cursor_guard);
            let Some(path) = selection.upgrade().map(|selection| selection.path.clone()) else {
                return;
            };
            let texture = crate::ui::image::texture(path, (-1, -1)).await;
            let Some(selection) = selection.upgrade() else {
                return;
            };
            selection.screenshot_texture.replace(texture);
            selection.show_mode_region();
            selection.window.set_visible(true);
            selection.show_description();
            selection.rise();
        });
    }

    fn build(self: &Rc<Self>, monitor: &gdk::Monitor) -> Rc<Selection> {
        let screen: String = monitor.connector().map(Into::into).unwrap_or_default();
        let geometry = monitor.geometry();
        let scale = monitor.scale();
        let offset = hypr::json("monitors")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .find(|entry| entry.get("name").and_then(Value::as_str) == Some(screen.as_str()))
            .map(|entry| {
                (
                    entry.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                    entry.get("y").and_then(Value::as_f64).unwrap_or(0.0),
                )
            })
            .unwrap_or((geometry.x() as f64, geometry.y() as f64));
        let (windows, layers) = targets(&screen, offset);

        let paint = Paint::new(|_, _, _| {});
        paint.set_hexpand(true);
        paint.set_vexpand(true);
        paint.set_cursor_from_name(Some("crosshair"));

        let symbol = text::symbol(guide_symbol(Action::Copy), GUIDE_ICON);
        text::set_color(&symbol, "colOnPrimary");
        let (symbol_holder, set_symbol) = text::animate_change(&symbol);
        let description_label = text::styled(&tr(guide_description(Action::Copy)));
        text::set_color(&description_label, "colOnPrimary");
        description_label.set_margin_start(GUIDE_SPACING - GUIDE_TEXT_INSET);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.set_halign(gtk4::Align::Start);
        row.set_valign(gtk4::Align::Center);
        row.set_margin_start(GUIDE_PADDING);
        row.append(&Centered::new(&symbol_holder));
        row.append(&Centered::new(&description_label));

        let background = Paint::new({
            let theme = self.theme.clone();
            move |snapshot, width, height| {
                let color = theme.borrow().colors.col_primary;
                let bounds = graphene::Rect::new(0.0, 0.0, width, height);
                let full_radius = (height / 2.0).min(width / 2.0);
                let size = |radius: f32| graphene::Size::new(radius, radius);
                let corner_radius = (GUIDE_CORNER as f32).min(full_radius);
                let outline = gsk::RoundedRect::new(
                    bounds,
                    size(corner_radius),
                    size(full_radius),
                    size(full_radius),
                    size(full_radius),
                );
                snapshot.push_rounded_clip(&outline);
                snapshot.append_color(&color, &bounds);
                snapshot.pop();
            }
        });
        let guide_widget = gtk4::Overlay::new();
        guide_widget.set_child(Some(&background));
        guide_widget.add_overlay(&row);
        guide_widget.set_overflow(gtk4::Overflow::Hidden);
        guide_widget.set_can_target(false);

        let layer = gtk4::Fixed::new();
        layer.set_can_target(false);
        layer.put(&guide_widget, 0.0, 0.0);

        let mode_names = MODES.map(|(_, _, name)| tr(name));
        let entries: Vec<(&str, &str)> = MODES
            .iter()
            .zip(&mode_names)
            .map(|((_, icon, _), name)| (*icon, name.as_str()))
            .collect();
        let tabs = TabBar::new(&self.theme, &entries);
        let tune_symbol = text::symbol("tune", TOOLBAR_ICON);
        text::set_color(&tune_symbol, "m3onBackground");
        let options_label = text::styled(&tr("Options"));
        let arrow_symbol = text::symbol("keyboard_arrow_up", ARROW_ICON);
        text::set_color(&arrow_symbol, "m3onBackground");
        let options_row = gtk4::Box::new(gtk4::Orientation::Horizontal, OPTIONS_SPACING);
        options_row.append(&Centered::integral(&tune_symbol));
        options_row.append(&Centered::new(&options_label));
        options_row.append(&Centered::integral(&arrow_symbol));
        let options_button = toolbar::button(&self.theme, &options_row);
        let toolbar_frame = toolbar::frame();
        toolbar_frame.append(&tabs.widget);
        toolbar_frame.append(&toolbar::separator());
        toolbar_frame.append(&options_button);
        let (capture_button, capture_symbol, capture_tip) =
            toolbar::paired_fab(&self.theme, "photo_camera", &tr("Capture"));
        let (close_button, _, _) = toolbar::paired_fab(&self.theme, "close", &tr("Close"));
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, CONTROLS_SPACING);
        row.append(&toolbar_frame);
        row.append(&capture_button);
        row.append(&close_button);
        let current_config = self.config();
        let (menu, switches, update_countdown) = options_menu(&self.theme, &current_config);
        menu.set_visible(false);
        let controls_column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        controls_column.append(&menu);
        controls_column.append(&row);
        let holder = text::Shift::new(&controls_column);
        holder.set_halign(gtk4::Align::Center);
        holder.set_valign(gtk4::Align::End);
        holder.set_margin_bottom(CONTROLS_MARGIN);
        holder.set_opacity(0.0);

        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&paint));
        overlay.add_overlay(&layer);
        overlay.add_overlay(&holder);

        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .child(&overlay)
            .build();
        window.add_css_class("region-selector");
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        window.set_monitor(Some(monitor));
        window.set_layer(Layer::Overlay);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::OnDemand);

        let fill_count = windows.len() + layers.len();
        let target_opacity = current_config.region.target_opacity;
        let selection = Rc::new(Selection {
            selector: Rc::downgrade(self),
            controls: Controls {
                holder,
                tabs,
                capture_button,
                capture_symbol,
                capture_tip,
                options_button,
                arrow_symbol,
                menu,
                is_open: Cell::new(false),
                rise_tween: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
                appear_tween: Cell::new(Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
                _switches: switches,
                _update_countdown: update_countdown,
            },
            config: current_config,
            theme: self.theme.clone(),
            window: window.clone(),
            paint: paint.clone(),
            layer,
            guide: Guide {
                widget: guide_widget,
                background,
                set_symbol,
                description_label,
                width_tween: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
                fade: Cell::new(Tween::new(1.0, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
                hide_timer: RefCell::new(None),
            },
            path: screenshot::temp_path(&screen),
            screen,
            size: (geometry.width() as f64, geometry.height() as f64),
            scale: scale as f64,
            offset,
            mode: Cell::new(self.mode.get()),
            phase: Cell::new(Phase::Select),
            screenshot_texture: RefCell::new(None),
            drag: RefCell::new(Drag::default()),
            region: Cell::new([0.0; 4]),
            target_bounds: Cell::new(None),
            windows,
            layers,
            fills: RefCell::new(vec![
                Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS);
                fill_count
            ]),
            fade: Cell::new(Tween::new(target_opacity, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
            button: Cell::new(0),
            pointer: Cell::new((0.0, 0.0)),
            action: Cell::new(Action::Copy),
            breath_start: Cell::new(0),
            is_ticking: Cell::new(false),
            has_snipped: Cell::new(false),
        });
        selection
            .controls
            .tabs
            .set_current(mode_index(selection.mode.get()));
        selection.sync_capture();
        selection.controls.tabs.connect_selected({
            let selection = Rc::downgrade(&selection);
            move |index| {
                if let Some(selection) = selection.upgrade() {
                    selection.set_mode(MODES[index].0);
                }
            }
        });
        selection.controls.options_button.connect_clicked({
            let selection = Rc::downgrade(&selection);
            move |_| {
                if let Some(selection) = selection.upgrade() {
                    selection.toggle_options();
                }
            }
        });
        selection.controls.capture_button.connect_clicked({
            let selection = Rc::downgrade(&selection);
            move |_| {
                if let Some(selection) = selection.upgrade() {
                    selection.capture_whole_screen();
                }
            }
        });
        close_button.connect_clicked({
            let selection = Rc::downgrade(&selection);
            move |_| {
                if let Some(selection) = selection.upgrade() {
                    selection.dismiss();
                }
            }
        });
        let action = selection.action();
        selection.action.set(action);
        (selection.guide.set_symbol)(guide_symbol(action));
        selection
            .guide
            .description_label
            .set_text(&tr(guide_description(action)));
        let mut width_tween = selection.guide.width_tween.get();
        width_tween.jump(selection.guide_width(true));
        selection.guide.width_tween.set(width_tween);
        selection.apply_guide(0);

        paint.set_draw({
            let selection = Rc::downgrade(&selection);
            move |snapshot, _, _| {
                if let Some(selection) = selection.upgrade() {
                    selection.draw(snapshot);
                }
            }
        });

        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_motion({
            let selection = Rc::downgrade(&selection);
            move |_, x, y| {
                if let Some(selection) = selection.upgrade()
                    && selection.phase.get() == Phase::Select
                {
                    selection.handle_motion(x, y);
                }
            }
        });
        paint.add_controller(motion_controller);

        let drag_gesture = gtk4::GestureDrag::new();
        drag_gesture.set_button(0);
        drag_gesture.connect_drag_begin({
            let selection = Rc::downgrade(&selection);
            move |gesture, x, y| {
                let button = gesture.current_button();
                if button != gdk::BUTTON_PRIMARY && button != gdk::BUTTON_SECONDARY {
                    return;
                }
                if let Some(selection) = selection.upgrade()
                    && selection.phase.get() == Phase::Select
                {
                    selection.pointer.set((x, y));
                    selection.press(button, x, y);
                }
            }
        });
        drag_gesture.connect_drag_end({
            let selection = Rc::downgrade(&selection);
            move |gesture, _, _| {
                let button = gesture.current_button();
                if button != gdk::BUTTON_PRIMARY && button != gdk::BUTTON_SECONDARY {
                    return;
                }
                if let Some(selection) = selection.upgrade()
                    && selection.phase.get() == Phase::Select
                {
                    selection.release();
                }
            }
        });
        paint.add_controller(drag_gesture);

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let selection = Rc::downgrade(&selection);
            move |_, key, _, _| {
                let Some(selection) = selection.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                match key {
                    gdk::Key::Escape => selection.dismiss(),
                    gdk::Key::space => {
                        let next_mode = if selection.mode.get() == Mode::Window {
                            Mode::Region
                        } else {
                            Mode::Window
                        };
                        selection.set_mode(next_mode);
                    }
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        });
        window.add_controller(key_controller);

        selection
    }
}

fn close(selection: &Rc<Selection>) {
    if let Some(source) = selection.guide.hide_timer.borrow_mut().take() {
        source.remove();
    }
    if !selection.has_snipped.get() {
        let _ = std::fs::remove_file(&selection.path);
    }
    selection.screenshot_texture.replace(None);
    selection.window.destroy();
    crate::ui::unload::trim();
}

async fn run(command: &[&str]) -> bool {
    process::finish(process::quiet(command)).await == Some(true)
}
