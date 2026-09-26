use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::services::Services;
use crate::services::session::Session;

const NAMESPACE: &str = "proscenio:session";
const SIZE: i32 = 120;
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
    for (index, (icon, label)) in entries.iter().enumerate() {
        let button = action(icon, label);
        button.connect_clicked({
            let session = services.session.clone();
            let window = window.clone();
            let label = *label;
            move |_| {
                run(&session, label);
                window.set_visible(false);
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
    dismiss.connect_pressed({
        let window = window.clone();
        move |_, _, _, _| window.set_visible(false)
    });
    stack.add_controller(dismiss);

    let escape = gtk4::EventControllerKey::new();
    escape.connect_key_pressed({
        let window = window.clone();
        move |_, key, _, _| {
            if key != gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            window.set_visible(false);
            glib::Propagation::Stop
        }
    });
    window.add_controller(escape);

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

fn action(icon: &str, label: &str) -> gtk4::Button {
    let symbol = gtk4::Label::new(Some(icon));
    symbol.add_css_class("icon");
    symbol.add_css_class("session-icon");

    let button = gtk4::Button::new();
    button.add_css_class("session-action");
    button.set_child(Some(&symbol));
    button.set_size_request(SIZE, SIZE);
    button.set_tooltip_text(Some(label));
    button
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
        let Ok(process) = gio::Subprocess::newv(
            &["bash", "-c", line].map(std::ffi::OsStr::new),
            gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
        ) else {
            return;
        };
        if process.wait_future().await.is_err() {
            return;
        }
        target.set_visible(process.has_exited() && process.exit_status() == 0);
    });
}
