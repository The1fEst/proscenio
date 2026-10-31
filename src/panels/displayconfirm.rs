use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::platform::hyprland::hypr;
use crate::services::hardware::displayconfirm::DisplayConfirmation;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const NAMESPACE: &str = "proscenio:displayConfirm";
const WIDTH: f64 = 400.0;
const ICON: f64 = 26.0;

struct Surface {
    window: gtk4::ApplicationWindow,
    description: gtk4::Label,
    _dialog: Rc<WindowDialog>,
}

pub struct DisplayConfirmWindows {
    app: gtk4::Application,
    theme: SharedTheme,
    confirmation: Rc<DisplayConfirmation>,
    surfaces: RefCell<Vec<Surface>>,
}

impl DisplayConfirmWindows {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme) -> Rc<Self> {
        let windows = Rc::new(DisplayConfirmWindows {
            app: app.clone(),
            theme: theme.clone(),
            confirmation: DisplayConfirmation::shared(),
            surfaces: RefCell::new(Vec::new()),
        });
        windows
            .confirmation
            .connect_changed({
                let windows = Rc::downgrade(&windows);
                move || {
                    if let Some(windows) = windows.upgrade() {
                        windows.sync();
                    }
                }
            })
            .detach();
        if let Some(display) = gdk::Display::default() {
            display.monitors().connect_items_changed({
                let windows = Rc::downgrade(&windows);
                move |_, _, _, _| {
                    if let Some(windows) = windows.upgrade() {
                        windows.reopen();
                    }
                }
            });
        }
        windows
    }

    fn sync(self: &Rc<Self>) {
        let Some(remaining_seconds) = self.confirmation.remaining_seconds() else {
            self.close();
            return;
        };
        if self.surfaces.borrow().is_empty() {
            self.open();
        }
        let description = match self.confirmation.restore_error() {
            Some(error) => trf("The previous settings could not be restored: %1", &[&error]),
            None => trf(
                "The previous settings come back in %1 s unless you keep these.",
                &[&remaining_seconds.to_string()],
            ),
        };
        for surface in self.surfaces.borrow().iter() {
            surface.description.set_text(&description);
        }
    }

    fn reopen(self: &Rc<Self>) {
        if self.surfaces.borrow().is_empty() {
            return;
        }
        self.close();
        self.sync();
    }

    fn close(&self) {
        for surface in self.surfaces.take() {
            surface.window.destroy();
        }
    }

    fn open(self: &Rc<Self>) {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let mut monitors: Vec<gdk::Monitor> = display
            .monitors()
            .iter::<gdk::Monitor>()
            .flatten()
            .collect();
        let focused_connector = hypr::focused_monitor();
        if let Some(index) = monitors
            .iter()
            .position(|monitor| monitor.connector().map(String::from) == focused_connector)
        {
            monitors[..=index].rotate_right(1);
        }
        let surfaces = monitors
            .iter()
            .enumerate()
            .map(|(index, monitor)| self.surface(monitor, index == 0))
            .collect();
        self.surfaces.replace(surfaces);
    }

    fn surface(self: &Rc<Self>, monitor: &gdk::Monitor, takes_keyboard: bool) -> Surface {
        let dialog = WindowDialog::new(&self.theme, None);
        dialog.set_background_width(WIDTH);
        dialog.set_scrim_radius(0.0);

        let icon = text::symbol("desktop_windows", ICON);
        text::set_color(&icon, "colSecondary");
        dialog.column.add(&Centered::integral(&icon), Place::wide());

        let title = windowdialog::title(&tr("Keep these display settings?"));
        title.set_xalign(0.5);
        title.set_justify(gtk4::Justification::Center);
        dialog.column.add(&title, Place::wide());

        let description = text::styled("");
        text::set_color(&description, "colOnSurfaceVariant");
        description.set_wrap(true);
        description.set_xalign(0.5);
        description.set_justify(gtk4::Justification::Center);
        dialog.column.add(&description, Place::wide());

        let (row, place) = windowdialog::button_row();
        let revert_button = windowdialog::button(&self.theme, &tr("Revert"));
        let keep_button = windowdialog::button(&self.theme, &tr("Keep"));
        row.append(&windowdialog::spacer());
        row.append(&revert_button);
        row.append(&keep_button);
        dialog.column.add(&row, place);

        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .child(&dialog.root)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        window.set_monitor(Some(monitor));
        window.set_layer(Layer::Overlay);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(if takes_keyboard {
            KeyboardMode::Exclusive
        } else {
            KeyboardMode::None
        });

        keep_button.connect_clicked({
            let confirmation = self.confirmation.clone();
            move |_| confirmation.keep()
        });
        revert_button.connect_clicked({
            let confirmation = self.confirmation.clone();
            move |_| confirmation.revert()
        });
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let confirmation = self.confirmation.clone();
            move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                confirmation.revert();
                glib::Propagation::Stop
            }
        });
        window.add_controller(key_controller);

        window.present();
        dialog.show(true, || {});
        Surface {
            window,
            description,
            _dialog: dialog,
        }
    }
}
