use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config::{self, Config};
use crate::core::process;
use crate::platform::{hypr, ipc};
use crate::ui::theme::Theme;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

pub const COMMAND: &str = "renderer-check";
const APP_ID: &str = "dev.fEst.Proscenio.RendererCheck";
const NAMESPACE: &str = "proscenio:rendererCheck";
const SAFE_RENDERER: &str = "cairo";
const SECONDS: u32 = 15;
const WIDTH: f64 = 400.0;
const ICON: f64 = 26.0;
const BUTTON_ROW_BOTTOM: f64 = 10.0;

pub fn start_if_on_trial() {
    if config::renderer_fallback().is_none() {
        return;
    }
    let launcher = process::own_session(
        gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
    );
    launcher.setenv("GSK_RENDERER", SAFE_RENDERER, true);
    let executable = process::executable();
    let _ = launcher.spawn(&[OsStr::new(&executable), OsStr::new(COMMAND)]);
}

pub fn run(shell: &'static str) -> glib::ExitCode {
    let Some(fallback) = config::renderer_fallback() else {
        return glib::ExitCode::SUCCESS;
    };
    let trying = config::value_str(config::RENDERER).unwrap_or_default();
    let app = gtk4::Application::builder().application_id(APP_ID).build();
    app.connect_activate(move |app| ask(app, shell, &trying, &fallback));
    app.run_with_args(&[] as &[&str])
}

fn ask(app: &gtk4::Application, shell: &'static str, trying: &str, fallback: &str) {
    let config = Config::load();
    text::init(&config);
    let theme = Rc::new(RefCell::new(Theme::load(&config)));
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&theme.borrow().css());
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
    }

    let dialog = WindowDialog::new(&theme, None);
    dialog.set_background_width(WIDTH);
    let icon = text::symbol("brush", ICON);
    text::set_color(&icon, "colSecondary");
    dialog.column.add(&Centred::integral(&icon), Place::wide());
    let title = windowdialog::title(&format!("Keep the {} renderer?", name(trying)));
    title.set_xalign(0.5);
    title.set_justify(gtk4::Justification::Center);
    dialog.column.add(&title, Place::wide());
    let description = text::styled("");
    text::set_color(&description, "colOnSurfaceVariant");
    description.set_wrap(true);
    description.set_xalign(0.5);
    description.set_justify(gtk4::Justification::Center);
    dialog.column.add(&description, Place::wide());
    let (row, mut place) = windowdialog::button_row();
    place.bottom = BUTTON_ROW_BOTTOM;
    let revert = windowdialog::button(&theme, "Revert");
    let keep = windowdialog::button(&theme, "Keep");
    row.append(&windowdialog::spacer());
    row.append(&revert);
    row.append(&keep);
    dialog.column.add(&row, place);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&dialog.root)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    let focused = hypr::focused_monitor();
    let monitor = gdk::Display::default().and_then(|display| {
        display
            .monitors()
            .iter::<gdk::Monitor>()
            .flatten()
            .find(|monitor| monitor.connector().map(String::from) == focused)
    });
    window.set_monitor(monitor.as_ref());
    window.set_layer(Layer::Overlay);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let fallback_name = name(fallback);
    let left = Rc::new(Cell::new(SECONDS));
    let show_left = {
        let description = description.clone();
        let left = left.clone();
        move || {
            description.set_text(&format!(
                "{fallback_name} comes back in {} s unless you keep this one.",
                left.get()
            ));
        }
    };
    show_left();

    let settled = Rc::new(Cell::new(false));
    let settle: Rc<dyn Fn(bool)> = Rc::new({
        let (app, settled) = (app.clone(), settled.clone());
        move |kept| {
            if settled.replace(true) {
                return;
            }
            let answer = if kept { "keep" } else { "revert" };
            if !call_shell(shell, answer) {
                config::settle_renderer(kept);
                if !kept {
                    start_shell();
                }
            }
            app.quit();
        }
    });
    glib::timeout_add_local(Duration::from_secs(1), {
        let settle = settle.clone();
        move || {
            left.set(left.get().saturating_sub(1));
            if left.get() == 0 {
                settle(false);
                return glib::ControlFlow::Break;
            }
            show_left();
            glib::ControlFlow::Continue
        }
    });
    keep.connect_clicked({
        let settle = settle.clone();
        move |_| settle(true)
    });
    revert.connect_clicked({
        let settle = settle.clone();
        move |_| settle(false)
    });
    dialog.connect_dismiss(move || settle(false));

    window.present();
    dialog.show(true, || {});
    std::mem::forget(dialog);
}

fn call_shell(shell: &str, answer: &str) -> bool {
    let arguments = ["call".to_owned(), "renderer".to_owned(), answer.to_owned()];
    ipc::client(shell, &arguments) == glib::ExitCode::SUCCESS
}

fn start_shell() {
    let launcher = process::own_session(
        gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
    );
    launcher.unsetenv("GSK_RENDERER");
    let executable = process::executable();
    let _ = launcher.spawn(&[OsStr::new(&executable)]);
}

fn name(renderer: &str) -> String {
    match renderer {
        "cairo" => "Cairo".to_owned(),
        "opengl" | "ngl" | "gl" => "OpenGL".to_owned(),
        "vulkan" => "Vulkan".to_owned(),
        other => other.to_owned(),
    }
}
