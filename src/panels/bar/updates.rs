use gtk4::prelude::*;

use crate::core::scope::Scope;
use crate::core::watch;
use crate::services::updates::Updates;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::{display::text, overlay::reveal};

pub fn build(updates: &Updates, is_vertical: bool, scope: &Scope) -> gtk4::Widget {
    let label = text::symbol("deployed_code_update", pixel_size::LARGER as f64);
    label.set_valign(gtk4::Align::Center);

    let should_show = |updates: &Updates| updates.is_shell_behind() || updates.is_upgrade_advised();
    let (holder, set) = if is_vertical {
        label.set_halign(gtk4::Align::Center);
        reveal::vertical(
            &label,
            should_show(updates),
            crate::panels::bar::VERTICAL_INDICATOR_SPACING as f64,
        )
    } else {
        reveal::wrap(&label, should_show(updates))
    };
    let show = {
        let updates = updates.clone();
        move || {
            set(should_show(&updates));
            if updates.is_upgrade_strongly_advised() {
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
