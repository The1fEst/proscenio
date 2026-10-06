use gtk4::prelude::*;

use crate::core::scope::Scope;
use crate::core::watch;
use crate::services::updates::Updates;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::{reveal, text};

pub fn build(updates: &Updates, vertical: bool, scope: &Scope) -> gtk4::Widget {
    let label = text::symbol("deployed_code_update", pixel_size::LARGER as f64);
    label.set_valign(gtk4::Align::Center);

    let shown = |updates: &Updates| updates.shell_behind() || updates.advised();
    let (holder, set) = if vertical {
        label.set_halign(gtk4::Align::Center);
        reveal::vertical(
            &label,
            shown(updates),
            crate::panels::bar::VERTICAL_INDICATOR_SPACING as f64,
        )
    } else {
        reveal::wrap(&label, shown(updates))
    };
    let show = {
        let updates = updates.clone();
        move || {
            set(shown(&updates));
            if updates.strongly_advised() {
                label.add_css_class("urgent");
            } else {
                label.remove_css_class("urgent");
            }
        }
    };
    show();
    scope.keep(updates.subscribe(show.clone()));
    scope.hold(watch::config("/updates", show));

    holder.upcast()
}
