use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::process;
use crate::core::scope::Scope;
use crate::services::Services;
use crate::services::session::Session;
use crate::ui::anim::EXPRESSIVE_EFFECTS;
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton, Token};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

const NAMESPACE: &str = "proscenio:session";
const SIZE: i32 = 120;
const RADIUS: f64 = rounding::VERYLARGE as f64;
const FOCUSED_RADIUS: f64 = SIZE as f64 / 2.0;
const RADIUS_MILLIS: f64 = 200.0;
const ICON_SIZE: f64 = 45.0;
const COLUMNS: i32 = 4;
const SPACING: i32 = 15;

const ACTIONS: [(&str, &str); 7] = [
    ("lock", "Lock"),
    ("dark_mode", "Sleep"),
    ("logout", "Logout"),
    ("desktop_windows", "Reboot to Windows"),
    ("downloading", "Hibernate"),
    ("power_settings_new", "Shutdown"),
    ("restart_alt", "Reboot"),
];

const FIRMWARE: (&str, &str) = ("settings_applications", "Reboot to firmware settings");

pub struct SessionScreen {
    pub window: gtk4::ApplicationWindow,
    probe_warnings: Rc<dyn Fn()>,
}

impl SessionScreen {
    pub fn open(&self) {
        if !self.window.is_visible() {
            self.toggle();
        }
    }

    pub fn close(&self) {
        self.window.set_visible(false);
    }

    pub fn toggle(&self) {
        if self.window.is_visible() {
            self.window.set_visible(false);
            return;
        }
        (self.probe_warnings)();
        self.window.set_visible(true);
    }
}

pub fn build(
    app: &gtk4::Application,
    theme: &SharedTheme,
    services: &Rc<Services>,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Rc<SessionScreen> {
    let title = gtk4::Label::new(Some(&tr("Session")));
    title.add_css_class("session-title");
    let hint = gtk4::Label::new(Some(&tr(
        "Arrow keys to navigate, Enter to select\nEsc or click anywhere to cancel",
    )));
    hint.add_css_class("session-hint");
    hint.set_justify(gtk4::Justification::Center);

    let head = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    head.set_halign(gtk4::Align::Center);
    head.append(&title);
    head.append(&hint);

    let subtitle = gtk4::Label::new(None);
    subtitle.add_css_class("session-subtitle");
    let caption = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    caption.add_css_class("session-caption");
    caption.set_halign(gtk4::Align::Center);
    caption.append(&subtitle);

    let grid = gtk4::Grid::new();
    grid.set_halign(gtk4::Align::Center);
    grid.set_row_spacing(SPACING as u32);
    grid.set_column_spacing(SPACING as u32);

    let window = gtk4::ApplicationWindow::builder().application(app).build();

    let mut entries: Vec<(&str, &str)> = ACTIONS.to_vec();
    entries.push(FIRMWARE);
    let mut actions = Vec::new();
    for (index, (icon, label)) in entries.iter().enumerate() {
        let action = SessionAction::new(theme, icon, label);
        let button = action.button.clone();
        actions.push(action);
        button.connect_clicked({
            let session = services.session.clone();
            let window = window.downgrade();
            let label = *label;
            move |_| {
                run(&session, label);
                if let Some(window) = window.upgrade() {
                    window.set_visible(false);
                }
            }
        });
        let focus_controller = gtk4::EventControllerFocus::new();
        focus_controller.connect_enter({
            let subtitle = subtitle.clone();
            let label = tr(label);
            move |_| subtitle.set_text(&label)
        });
        button.add_controller(focus_controller);
        grid.attach(
            &button,
            index as i32 % COLUMNS,
            index as i32 / COLUMNS,
            1,
            1,
        );
    }

    let downloads_warning = warning(&tr(
        "There might be a download in progress. Check your Downloads folder.",
    ));
    let packages_warning = warning(&tr("Your package manager is running"));
    let notes = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    notes.set_halign(gtk4::Align::Center);
    notes.set_margin_top(10);
    notes.append(&downloads_warning);
    notes.append(&packages_warning);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);
    column.set_halign(gtk4::Align::Center);
    column.set_valign(gtk4::Align::Center);
    column.set_vexpand(true);
    column.append(&head);
    column.append(&grid);
    column.append(&caption);

    let stack = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    stack.add_css_class("session-screen");
    stack.set_vexpand(true);
    stack.append(&column);
    stack.append(&notes);
    notes.set_valign(gtk4::Align::Start);

    window.set_child(Some(&stack));
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Overlay);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_visible(false);

    let dismiss_gesture = gtk4::GestureClick::new();
    dismiss_gesture.connect_pressed(|gesture, _, x, y| {
        let Some(stack) = gesture.widget() else {
            return;
        };
        let is_on_button = stack
            .pick(x, y, gtk4::PickFlags::DEFAULT)
            .and_then(|target| target.ancestor(RippleButton::static_type()))
            .is_some();
        if !is_on_button && let Some(window) = stack.root().and_downcast::<gtk4::Window>() {
            window.set_visible(false);
        }
    });
    stack.add_controller(dismiss_gesture);

    let actions = Rc::new(actions);
    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    key_controller.connect_key_pressed({
        let actions = actions.clone();
        move |controller, key, _, _| {
            if key == gdk::Key::Escape {
                if let Some(window) = controller.widget() {
                    window.set_visible(false);
                }
                return glib::Propagation::Stop;
            }
            let Some(index) = actions.iter().position(|action| action.is_focused.get()) else {
                return glib::Propagation::Proceed;
            };
            let columns = COLUMNS as usize;
            let (column, row) = (index % columns, index / columns);
            let target = match key {
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    let action = &actions[index];
                    action.is_key_down.set(true);
                    action.show();
                    action.button.emit_clicked();
                    return glib::Propagation::Stop;
                }
                gdk::Key::Left => (column > 0).then(|| index - 1),
                gdk::Key::Right => (column + 1 < columns).then(|| index + 1),
                gdk::Key::Up => (row > 0).then(|| index - columns),
                gdk::Key::Down => Some(index + columns).filter(|below| *below < actions.len()),
                _ => return glib::Propagation::Proceed,
            };
            if let Some(target) = target {
                actions[target].button.grab_focus();
            }
            glib::Propagation::Stop
        }
    });
    key_controller.connect_key_released({
        let actions = actions.clone();
        move |_, key, _, _| {
            if !matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
                return;
            }
            for action in actions.iter() {
                if action.is_key_down.replace(false) {
                    action.show();
                }
            }
        }
    });
    window.add_controller(key_controller);
    window.connect_visible_notify({
        let actions = actions.clone();
        move |window| {
            if !window.is_visible() {
                return;
            }
            for action in actions.iter() {
                action.is_key_down.set(false);
            }
            if let Some(first) = actions.first() {
                first.button.grab_focus();
            }
            for action in actions.iter() {
                action.show();
            }
        }
    });

    let probe_warnings: Rc<dyn Fn()> = {
        let downloads_warning = downloads_warning.clone();
        let packages_warning = packages_warning.clone();
        Rc::new(move || {
            downloads_warning.set_visible(false);
            packages_warning.set_visible(false);
            probe(
                "pidof curl wget aria2c yt-dlp || ls ~/Downloads | grep -E '\\.crdownload$|\\.part$'",
                downloads_warning.clone(),
            );
            probe(
                "pidof yay paru dnf zypper apt apx xbps snap apk yum epsi pikman \
                 || ls /var/lib/pacman/db.lck",
                packages_warning.clone(),
            );
        })
    };

    let screen = Rc::new(SessionScreen {
        window,
        probe_warnings,
    });
    scope.keep(services.states.subscribe({
        let screen = Rc::downgrade(&screen);
        let states = services.states.clone();
        move || {
            if let Some(screen) = screen.upgrade()
                && states.is_screen_locked.get()
            {
                screen.close();
            }
        }
    }));
    screen
}

fn run(session: &Session, label: &str) {
    match label {
        "Lock" => session.lock(),
        "Sleep" => session.suspend(),
        "Logout" => session.logout(),
        "Reboot to Windows" => session.reboot_to_windows(),
        "Hibernate" => session.hibernate(),
        "Shutdown" => session.poweroff(),
        "Reboot" => session.reboot(),
        _ => session.reboot_to_firmware(),
    }
}

struct SessionAction {
    button: RippleButton,
    icon: gtk4::Label,
    is_focused: Cell<bool>,
    is_hovered: Cell<bool>,
    is_key_down: Cell<bool>,
}

impl SessionAction {
    fn new(theme: &SharedTheme, icon: &str, label: &str) -> Rc<Self> {
        let symbol = text::symbol(icon, ICON_SIZE);
        let button = RippleButton::new(theme);
        button.set_size_request(SIZE, SIZE);
        button.set_radius(RADIUS);
        button.animate_radius(RADIUS_MILLIS, EXPRESSIVE_EFFECTS);
        button.set_content(&Centered::new(&symbol), 0, 0);
        let tip = Tooltip::new(&button, theme, tooltip::Kind::Styled);
        tip.set_text(&tr(label));
        tooltip::hover_delay(&button, &tip, 0);

        let action = Rc::new(SessionAction {
            button,
            icon: symbol,
            is_focused: Cell::new(false),
            is_hovered: Cell::new(false),
            is_key_down: Cell::new(false),
        });
        let refresh = {
            let action = Rc::downgrade(&action);
            move || {
                if let Some(action) = action.upgrade() {
                    action.show();
                }
            }
        };
        let focus_controller = gtk4::EventControllerFocus::new();
        let focus_handler_for = |is_focused: bool| {
            let action = Rc::downgrade(&action);
            move |_: &gtk4::EventControllerFocus| {
                if let Some(action) = action.upgrade() {
                    action.is_focused.set(is_focused);
                    action.show();
                }
            }
        };
        focus_controller.connect_enter(focus_handler_for(true));
        focus_controller.connect_leave(focus_handler_for(false));
        action.button.add_controller(focus_controller);
        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_enter({
            let action = Rc::downgrade(&action);
            move |_, _, _| {
                if let Some(action) = action.upgrade() {
                    action.is_hovered.set(true);
                    action.show();
                }
            }
        });
        motion_controller.connect_leave({
            let action = Rc::downgrade(&action);
            move |_| {
                if let Some(action) = action.upgrade() {
                    action.is_hovered.set(false);
                    action.show();
                }
            }
        });
        action.button.add_controller(motion_controller);
        action.button.connect_down(refresh.clone());
        action.button.connect_release(refresh);
        action.show();
        action
    }

    fn show(&self) {
        let is_focused = self.is_focused.get();
        let is_down = self.button.is_down();
        let is_key_down = self.is_key_down.get();
        self.button.set_radius(if is_focused || is_down {
            FOCUSED_RADIUS
        } else {
            RADIUS
        });
        let background: Token = if is_key_down {
            |theme| theme.colors.col_secondary_container_active
        } else if is_focused {
            |theme| theme.colors.col_primary
        } else {
            |theme| theme.colors.col_secondary_container
        };
        self.button.set_look(Look {
            background,
            hover: |theme| theme.colors.col_primary,
            toggled: background,
            toggled_hover: |theme| theme.colors.col_primary,
            ripple: |theme| theme.colors.col_primary_active,
            ripple_toggled: |theme| theme.colors.col_primary_active,
        });
        let is_lit = is_down || is_key_down || is_focused || self.is_hovered.get();
        text::set_color(
            &self.icon,
            if is_lit { "m3onPrimary" } else { "colOnLayer0" },
        );
    }
}

fn warning(text: &str) -> gtk4::Widget {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("session-warning-text");

    let holder = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    holder.add_css_class("session-warning");
    holder.set_halign(gtk4::Align::Center);
    holder.append(&label);
    holder.set_visible(false);
    holder.upcast()
}

fn probe(line: &'static str, target: gtk4::Widget) {
    glib::spawn_future_local(async move {
        let Some(is_success) = process::finish(process::quiet(&["bash", "-c", line])).await else {
            return;
        };
        target.set_visible(is_success);
    });
}
