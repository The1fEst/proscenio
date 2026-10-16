use gtk4::gdk;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::panels::mixer::VolumeMixer;
use crate::services::media::audio::Audio;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::ring::Ring;

const SPACING: i32 = 4;

pub fn build(
    audio: &Audio,
    theme: &SharedTheme,
    volume_mixer: &Rc<VolumeMixer>,
    is_vertical: bool,
    scope: &Scope,
) -> gtk4::Widget {
    let ring = Rc::new(Ring::new(theme.clone(), "tune"));

    let holder = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    if is_vertical {
        ring.area.set_hexpand(true);
        ring.area.set_halign(gtk4::Align::Center);
    } else {
        holder.set_valign(gtk4::Align::Center);
        ring.area.set_margin_end(SPACING * 2);
    }
    holder.append(&ring.area);

    let refresh = {
        let audio = audio.clone();
        let ring = ring.clone();
        move || ring.set(audio.sink_volume.get(), false)
    };
    refresh();
    scope.keep(audio.subscribe(refresh));

    let click = gtk4::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    click.connect_pressed({
        let volume_mixer = Rc::downgrade(volume_mixer);
        move |_, _, _, _| {
            if let Some(volume_mixer) = volume_mixer.upgrade() {
                volume_mixer.toggle();
            }
        }
    });
    holder.add_controller(click);

    volume_mixer.set_chip(&holder);
    holder.upcast()
}
