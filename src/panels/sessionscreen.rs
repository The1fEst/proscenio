use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::Cell;
use std::rc::Rc;

use crate::core::process;
use crate::core::scope::Scope;
use crate::services::Services;
use crate::services::session::Session;
use crate::ui::anim::EXPRESSIVE_EFFECTS;
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton, Token};
use crate::ui::widgets::text;
use crate::ui::widgets::tooltip::{self, Tooltip};

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
    warnings: Rc<dyn Fn()>,
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
        (self.warnings)();
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
    let title = gtk4::Label::new(Some("Session"));
    title.add_css_class("session-title");
    let hint = gtk4::Label::new(Some(
        "Arrow keys to navigate, Enter to select\nEsc or click anywhere to cancel",
    ));
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
        let focus = gtk4::EventControllerFocus::new();
        focus.connect_enter({
            let subtitle = subtitle.clone();
            let label = *label;
            move |_| subtitle.set_text(label)
        });
        button.add_controller(focus);
        grid.attach(
            &button,
            index as i32 % COLUMNS,
            index as i32 / COLUMNS,
            1,
            1,
        );
    }

    let downloads = warning("There might be a download in progress. Check your Downloads folder.");
    let packages = warning("Your package manager is running");
    let notes = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    notes.set_halign(gtk4::Align::Center);
    notes.set_margin_top(10);
    notes.append(&downloads);
    notes.append(&packages);

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

    let dismiss = gtk4::GestureClick::new();
    dismiss.connect_pressed(|gesture, _, x, y| {
        let Some(stack) = gesture.widget() else {
            return;
        };
        let on_button = stack
            .pick(x, y, gtk4::PickFlags::DEFAULT)
            .and_then(|target| target.ancestor(RippleButton::static_type()))
            .is_some();
        if !on_button && let Some(window) = stack.root().and_downcast::<gtk4::Window>() {
            window.set_visible(false);
        }
    });
    stack.add_controller(dismiss);

    let actions = Rc::new(actions);
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    keys.connect_key_pressed({
        let actions = actions.clone();
        move |keys, key, _, _| {
            if key == gdk::Key::Escape {
                if let Some(window) = keys.widget() {
                    window.set_visible(false);
                }
                return glib::Propagation::Stop;
            }
            let Some(index) = actions.iter().position(|action| action.focused.get()) else {
                return glib::Propagation::Proceed;
            };
            let columns = COLUMNS as usize;
            let (column, row) = (index % columns, index / columns);
            let target = match key {
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    let action = &actions[index];
                    action.keyboard_down.set(true);
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
    keys.connect_key_released({
        let actions = actions.clone();
        move |_, key, _, _| {
            if !matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
                return;
            }
            for action in actions.iter() {
                if action.keyboard_down.replace(false) {
                    action.show();
                }
            }
        }
    });
    window.add_controller(keys);
    window.connect_visible_notify({
        let actions = actions.clone();
        move |window| {
            if !window.is_visible() {
                return;
            }
            for action in actions.iter() {
                action.keyboard_down.set(false);
            }
            if let Some(first) = actions.first() {
                first.button.grab_focus();
            }
            for action in actions.iter() {
                action.show();
            }
        }
    });

    let warnings: Rc<dyn Fn()> = {
        let downloads = downloads.clone();
        let packages = packages.clone();
        Rc::new(move || {
            downloads.set_visible(false);
            packages.set_visible(false);
            probe(
                "pidof curl wget aria2c yt-dlp || ls ~/Downloads | grep -E '\\.crdownload$|\\.part$'",
                downloads.clone(),
            );
            probe(
                "pidof yay paru dnf zypper apt apx xbps snap apk yum epsi pikman \
                 || ls /var/lib/pacman/db.lck",
                packages.clone(),
            );
        })
    };

    let screen = Rc::new(SessionScreen { window, warnings });
    scope.keep(services.states.subscribe({
        let screen = Rc::downgrade(&screen);
        let states = services.states.clone();
        move || {
            if let Some(screen) = screen.upgrade()
                && states.screen_locked.get()
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
    focused: Cell<bool>,
    hovered: Cell<bool>,
    keyboard_down: Cell<bool>,
}

impl SessionAction {
    fn new(theme: &SharedTheme, icon: &str, label: &str) -> Rc<Self> {
        let symbol = text::symbol(icon, ICON_SIZE);
        let button = RippleButton::new(theme);
        button.set_size_request(SIZE, SIZE);
        button.set_radius(RADIUS);
        button.animate_radius(RADIUS_MILLIS, EXPRESSIVE_EFFECTS);
        button.set_content(&Centred::new(&symbol), 0, 0);
        let tip = Tooltip::new(&button, theme, tooltip::Kind::Styled);
        tip.set_text(label);
        tooltip::hover_delay(&button, &tip, 0);

        let action = Rc::new(SessionAction {
            button,
            icon: symbol,
            focused: Cell::new(false),
            hovered: Cell::new(false),
            keyboard_down: Cell::new(false),
        });
        let refresh = {
            let action = Rc::downgrade(&action);
            move || {
                if let Some(action) = action.upgrade() {
                    action.show();
                }
            }
        };
        let focus = gtk4::EventControllerFocus::new();
        let follow_focus = |focused: bool| {
            let action = Rc::downgrade(&action);
            move |_: &gtk4::EventControllerFocus| {
                if let Some(action) = action.upgrade() {
                    action.focused.set(focused);
                    action.show();
                }
            }
        };
        focus.connect_enter(follow_focus(true));
        focus.connect_leave(follow_focus(false));
        action.button.add_controller(focus);
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_enter({
            let action = Rc::downgrade(&action);
            move |_, _, _| {
                if let Some(action) = action.upgrade() {
                    action.hovered.set(true);
                    action.show();
                }
            }
        });
        motion.connect_leave({
            let action = Rc::downgrade(&action);
            move |_| {
                if let Some(action) = action.upgrade() {
                    action.hovered.set(false);
                    action.show();
                }
            }
        });
        action.button.add_controller(motion);
        action.button.connect_down(refresh.clone());
        action.button.connect_release(refresh);
        action.show();
        action
    }

    fn show(&self) {
        let focused = self.focused.get();
        let down = self.button.down();
        let keyboard_down = self.keyboard_down.get();
        self.button.set_radius(if focused || down {
            FOCUSED_RADIUS
        } else {
            RADIUS
        });
        let background: Token = if keyboard_down {
            |theme| theme.colors.col_secondary_container_active
        } else if focused {
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
        let lit = down || keyboard_down || focused || self.hovered.get();
        text::set_color(&self.icon, if lit { "m3onPrimary" } else { "colOnLayer0" });
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
        let Some(success) = process::finish(process::quiet(&["bash", "-c", line])).await else {
            return;
        };
        target.set_visible(success);
    });
}
