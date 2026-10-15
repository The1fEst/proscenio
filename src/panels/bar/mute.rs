use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::services::media::audio::Audio;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::{display::text, overlay::reveal};

pub fn build(audio: &Audio, is_vertical: bool, scope: &Scope) -> (gtk4::Widget, gtk4::Widget) {
    let sink = indicator(
        audio,
        "volume_off",
        &audio.is_sink_muted,
        is_vertical,
        scope,
    );
    let source = indicator(audio, "mic_off", &audio.is_source_muted, is_vertical, scope);
    (sink, source)
}

fn indicator(
    audio: &Audio,
    icon: &str,
    is_muted: &Rc<Cell<bool>>,
    is_vertical: bool,
    scope: &Scope,
) -> gtk4::Widget {
    let label = text::symbol(icon, pixel_size::LARGER as f64);
    label.set_valign(gtk4::Align::Center);

    let (holder, set) = if is_vertical {
        label.set_halign(gtk4::Align::Center);
        reveal::vertical(
            &label,
            is_muted.get(),
            crate::panels::bar::VERTICAL_INDICATOR_SPACING as f64,
        )
    } else {
        reveal::wrap(&label, is_muted.get())
    };
    scope.keep(audio.subscribe({
        let is_muted = is_muted.clone();
        move || set(is_muted.get())
    }));

    holder.upcast()
}
