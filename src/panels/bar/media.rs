use gtk4::gdk;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::services::mpris::Mpris;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::ring::Ring;

const SPACING: i32 = 4;

pub fn build(
    mpris: &Mpris,
    theme: &SharedTheme,
    open: impl Fn() + 'static,
    vertical: bool,
    scope: &Scope,
) -> gtk4::Widget {
    let ring = Rc::new(Ring::new(theme.clone(), "music_note"));

    let holder = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    if vertical {
        ring.area.set_hexpand(true);
        ring.area.set_halign(gtk4::Align::Center);
    } else {
        holder.set_valign(gtk4::Align::Center);
        ring.area.set_margin_end(SPACING * 2);
    }
    holder.append(&ring.area);

    let refresh = {
        let mpris = mpris.clone();
        let ring = ring.clone();
        move || {
            let active = mpris.active();
            let playing = active.as_ref().is_some_and(|track| track.playing);
            let length = active
                .as_ref()
                .map(|track| mpris.track_length(track))
                .unwrap_or(0);
            ring.set_icon(if playing { "pause" } else { "music_note" });
            ring.set(
                match &active {
                    Some(track) if length > 0 => track.position as f64 / length as f64,
                    _ => 0.0,
                },
                false,
            );
        }
    };
    refresh();
    scope.keep(mpris.subscribe(refresh));

    let click = gtk4::GestureClick::new();
    click.set_button(0);
    click.connect_pressed({
        let mpris = mpris.clone();
        move |gesture, _, _, _| {
            let button = gesture.current_button();
            if button == gdk::BUTTON_PRIMARY {
                open();
                return;
            }
            let Some(track) = mpris.active() else {
                return;
            };
            match button {
                gdk::BUTTON_MIDDLE => mpris.toggle_playing(&track),
                8 => mpris.previous(&track),
                gdk::BUTTON_SECONDARY | 9 => mpris.next(&track),
                _ => {}
            }
        }
    });
    holder.add_controller(click);

    holder.upcast()
}
