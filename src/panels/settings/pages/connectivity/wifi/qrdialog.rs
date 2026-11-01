use gtk4::glib;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::qr::{self, Code};
use crate::platform::network::nmprofile::wireless::{NetworkShare, Security};
use crate::services::network::nmsettings;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::icon_button;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const DIALOG_WIDTH: f64 = 380.0;
const CODE_SIZE: usize = 280;
const QUIET_ZONE: usize = 4;
const PASSWORD_SPACING: i32 = 8;
const HIDDEN_CHARACTER: &str = "•";

pub enum Source {
    Uuid(String),
    Id(String),
}

pub fn open(
    theme: &SharedTheme,
    present: Rc<dyn Fn(Rc<WindowDialog>)>,
    source: Source,
    trigger: &RippleButton,
) {
    trigger.set_sensitive(false);
    let trigger = trigger.downgrade();
    let theme = theme.clone();
    glib::spawn_future_local(async move {
        let share = read_share(source).await;
        if let Some(trigger) = trigger.upgrade() {
            trigger.set_sensitive(true);
        }
        present(build(&theme, share));
    });
}

async fn read_share(source: Source) -> Result<(NetworkShare, Code), String> {
    let uuid = match source {
        Source::Uuid(uuid) => uuid,
        Source::Id(id) => nmsettings::find_uuids(&id)
            .await
            .into_iter()
            .next()
            .ok_or_else(|| tr("This network cannot be put in a code"))?,
    };
    let stored_profile = nmsettings::load(&uuid).await?;
    let mut profile = stored_profile.profile;
    let has_password = matches!(
        profile.security(),
        Security::Personal | Security::Wpa3 | Security::Wep
    );
    if has_password && !stored_profile.are_secrets_read {
        for secrets in nmsettings::fetch_secrets(&uuid).await? {
            profile.merge_secrets(&secrets, true);
        }
    }
    let share = profile
        .to_network_share()
        .ok_or_else(|| tr("This network cannot be put in a code"))?;
    let code = qr::encode(share.payload.as_bytes())
        .ok_or_else(|| tr("This network cannot be put in a code"))?;
    Ok((share, code))
}

fn build(theme: &SharedTheme, share: Result<(NetworkShare, Code), String>) -> Rc<WindowDialog> {
    let dialog = WindowDialog::new(theme, None);
    dialog.set_background_width(DIALOG_WIDTH);
    match share {
        Ok((share, code)) => show_share(theme, &dialog, share, code),
        Err(message) => {
            dialog
                .column
                .add(&windowdialog::title(&tr("Share")), Place::wide());
            let problem = text::styled(&message);
            text::set_color(&problem, "colError");
            problem.set_wrap(true);
            problem.set_xalign(0.0);
            dialog.column.add(&problem, Place::wide());
        }
    }
    let (buttons, place) = windowdialog::button_row();
    buttons.append(&windowdialog::spacer());
    let close_button = windowdialog::button(theme, &tr("Close"));
    close_button.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    buttons.append(&close_button);
    dialog.column.add(&buttons, place);
    dialog
}

fn show_share(theme: &SharedTheme, dialog: &WindowDialog, share: NetworkShare, code: Code) {
    dialog
        .column
        .add(&windowdialog::title(&share.ssid), Place::wide());
    dialog.column.add(&draw_code(code), Place::wide());
    let caption = text::styled(&tr("Scan to join this network"));
    text::set_color(&caption, "colOnSurfaceVariant");
    caption.set_wrap(true);
    caption.set_xalign(0.5);
    caption.set_justify(gtk4::Justification::Center);
    dialog.column.add(&caption, Place::wide());
    if share.password.is_empty() {
        return;
    }

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, PASSWORD_SPACING);
    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_hexpand(true);
    lines.set_valign(gtk4::Align::Center);
    let name_label = text::styled_sized(&tr("Password"), pixel_size::SMALLER);
    text::set_color(&name_label, "colSubtext");
    name_label.set_xalign(0.0);
    lines.append(&name_label);
    let hidden_text = HIDDEN_CHARACTER.repeat(share.password.chars().count());
    let value_label = text::styled(&hidden_text);
    text::set_color(&value_label, "colOnSurface");
    value_label.set_xalign(0.0);
    value_label.set_wrap(true);
    value_label.set_wrap_mode(gtk4::pango::WrapMode::Char);
    lines.append(&value_label);
    row.append(&lines);

    let (reveal_button, _) = icon_button(theme, "visibility", false, "");
    let is_shown = Cell::new(false);
    reveal_button.connect_clicked(move |_| {
        is_shown.set(!is_shown.get());
        value_label.set_text(if is_shown.get() {
            &share.password
        } else {
            &hidden_text
        });
        value_label.set_selectable(is_shown.get());
    });
    row.append(&reveal_button);
    dialog.column.add(&row, Place::wide());
}

fn draw_code(code: Code) -> gtk4::DrawingArea {
    let module_count = code.size() + 2 * QUIET_ZONE;
    let side = (CODE_SIZE / module_count).max(1) * module_count;
    let area = gtk4::DrawingArea::new();
    area.set_content_width(side as i32);
    area.set_content_height(side as i32);
    area.set_halign(gtk4::Align::Center);
    area.set_draw_func(move |_, cr, width, height| {
        let module_side = (width.min(height) as usize / module_count).max(1) as f64;
        cr.set_source_rgb(1.0, 1.0, 1.0);
        let _ = cr.paint();
        cr.set_source_rgb(0.0, 0.0, 0.0);
        for y in 0..code.size() {
            for x in 0..code.size() {
                if code.is_dark(x, y) {
                    cr.rectangle(
                        (x + QUIET_ZONE) as f64 * module_side,
                        (y + QUIET_ZONE) as f64 * module_side,
                        module_side,
                        module_side,
                    );
                }
            }
        }
        let _ = cr.fill();
    });
    area
}
