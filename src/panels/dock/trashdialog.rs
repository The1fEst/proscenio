use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::platform::hyprland::hypr;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const NAMESPACE: &str = "proscenio:trashConfirm";
const WIDTH: f64 = 400.0;
const ICON: f64 = 26.0;

pub(super) fn confirm_empty(
    app: &gtk4::Application,
    theme: &SharedTheme,
    on_confirm: impl Fn() + 'static,
    on_closed: impl Fn() + 'static,
) {
    let dialog = WindowDialog::new(theme, None);
    dialog.set_background_width(WIDTH);

    let icon = text::symbol("delete_forever", ICON);
    text::set_color(&icon, "colError");
    dialog.column.add(&Centered::integral(&icon), Place::wide());

    let title = windowdialog::title(&tr("Empty the trash?"));
    title.set_xalign(0.5);
    title.set_justify(gtk4::Justification::Center);
    dialog.column.add(&title, Place::wide());

    let description = text::styled(&tr("Everything in the trash is deleted permanently."));
    text::set_color(&description, "colOnSurfaceVariant");
    description.set_wrap(true);
    description.set_xalign(0.5);
    description.set_justify(gtk4::Justification::Center);
    dialog.column.add(&description, Place::wide());

    let (row, place) = windowdialog::button_row();
    let cancel_button = windowdialog::button(theme, &tr("Cancel"));
    let empty_button = windowdialog::button(theme, &tr("Empty trash"));
    row.append(&windowdialog::spacer());
    row.append(&cancel_button);
    row.append(&empty_button);
    dialog.column.add(&row, place);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&dialog.root)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    let focused_connector = hypr::focused_monitor();
    let monitor = gdk::Display::default().and_then(|display| {
        display
            .monitors()
            .iter::<gdk::Monitor>()
            .flatten()
            .find(|monitor| monitor.connector().map(String::from) == focused_connector)
    });
    window.set_monitor(monitor.as_ref());
    window.set_layer(Layer::Overlay);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let owned_dialog = Rc::new(RefCell::new(Some(dialog.clone())));
    let has_responded = Rc::new(Cell::new(false));
    let respond: Rc<dyn Fn(bool)> = Rc::new({
        let dialog = Rc::downgrade(&dialog);
        let window = window.downgrade();
        move |is_confirmed| {
            if has_responded.replace(true) {
                return;
            }
            let Some(dialog) = dialog.upgrade() else {
                return;
            };
            let window = window.clone();
            let owned_dialog = owned_dialog.clone();
            dialog.show(false, move || {
                if let Some(window) = window.upgrade() {
                    window.destroy();
                }
                owned_dialog.take();
            });
            on_closed();
            if is_confirmed {
                on_confirm();
            }
        }
    });
    empty_button.connect_clicked({
        let respond = respond.clone();
        move |_| respond(true)
    });
    cancel_button.connect_clicked({
        let respond = respond.clone();
        move |_| respond(false)
    });
    dialog.connect_dismiss(move || respond(false));

    window.present();
    dialog.show(true, || {});
}
