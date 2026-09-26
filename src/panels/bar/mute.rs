use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::services::audio::Audio;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::{reveal, text};

pub fn build(audio: &Audio, vertical: bool, scope: &Scope) -> (gtk4::Widget, gtk4::Widget) {
    let sink = indicator(audio, "volume_off", &audio.sink_muted, vertical, scope);
    let source = indicator(audio, "mic_off", &audio.source_muted, vertical, scope);
    (sink, source)
}

fn indicator(
    audio: &Audio,
    icon: &str,
    muted: &Rc<Cell<bool>>,
    vertical: bool,
    scope: &Scope,
) -> gtk4::Widget {
    let label = text::symbol(icon, pixel_size::LARGER as f64);
    label.set_valign(gtk4::Align::Center);

    let (holder, set) = if vertical {
        label.set_halign(gtk4::Align::Center);
        reveal::vertical(
            &label,
            muted.get(),
            crate::panels::bar::VERTICAL_INDICATOR_SPACING as f64,
        )
    } else {
        reveal::wrap(&label, muted.get())
    };
    scope.keep(audio.subscribe({
        let muted = muted.clone();
        move || set(muted.get())
    }));

    holder.upcast()
}
