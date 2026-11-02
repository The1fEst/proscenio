use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const WIDTH: f64 = 340.0;

pub fn build(theme: &SharedTheme, on_confirm: impl Fn() + 'static) -> Rc<WindowDialog> {
    let dialog = WindowDialog::new(theme, None);
    dialog.set_background_width(WIDTH);
    dialog.column.add(
        &windowdialog::title(&tr("Delete this note?")),
        Place::wide(),
    );

    let description = text::styled(&tr("Its text is deleted permanently."));
    text::set_color(&description, "colOnSurfaceVariant");
    description.set_wrap(true);
    description.set_xalign(0.0);
    dialog.column.add(&description, Place::wide());

    let (row, place) = windowdialog::button_row();
    let cancel_button = windowdialog::button(theme, &tr("Cancel"));
    let delete_button = windowdialog::button(theme, &tr("Delete"));
    row.append(&windowdialog::spacer());
    row.append(&cancel_button);
    row.append(&delete_button);
    dialog.column.add(&row, place);

    cancel_button.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    delete_button.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            on_confirm();
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    dialog
}
