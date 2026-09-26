use gtk4::prelude::*;

use crate::core::scope::Scope;
use crate::services::recording::Recording;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::row::Row;
use crate::ui::widgets::{text, tooltip};

const HEIGHT: i32 = 26;
const HORIZONTAL_PADDING: i32 = 8;

pub fn build(recording: &Recording, theme: &SharedTheme, scope: &Scope) -> gtk4::Widget {
    let symbol = text::symbol_filled("stop_circle", pixel_size::LARGE as f64, 1.0);
    text::set_color(&symbol, "colOnError");
    let elapsed = text::styled_sized("0:00", pixel_size::SMALLER);
    text::set_color(&elapsed, "colOnError");

    let row = Row::new(4);
    row.append(&symbol);
    row.append(&text::shifted(&elapsed, -0.5));

    let button = RippleButton::new(theme);
    button.set_radius(rounding::FULL as f64);
    button.set_size_request(-1, HEIGHT);
    button.set_look(Look {
        background: |theme| theme.colors.col_error,
        hover: |theme| theme.colors.col_error_hover,
        ripple: |theme| theme.colors.col_error_active,
        ..Look::default()
    });
    button.set_content(&Centred::filling_width(&row), HORIZONTAL_PADDING, 0);
    button.connect_clicked({
        let recording = recording.clone();
        move |_| recording.stop()
    });

    let tip = tooltip::Tooltip::new(&button, theme, tooltip::Kind::Popup);
    tip.below();
    tip.set_text("Stop the recording");
    tooltip::hover_delay(&button, &tip, 0);

    let holder = Centred::new(&button);
    let show = {
        let recording = recording.clone();
        let holder = holder.clone();
        move || {
            holder.set_visible(recording.active());
            elapsed.set_text(&recording.elapsed());
        }
    };
    show();
    scope.keep(recording.subscribe(show));

    holder.upcast()
}
