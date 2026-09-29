use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::i18n::trf;
use crate::core::process::detach;
use crate::core::scope::Scope;
use crate::core::watch;
use crate::platform::desktop;
use crate::services::Services;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::{text, tooltip};

const SPACING: i32 = 4;
const SIZE: i32 = 26;

pub fn build(
    config: &Rc<Config>,
    services: &Services,
    theme: &SharedTheme,
    scope: &Scope,
) -> gtk4::Widget {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
    row.set_valign(gtk4::Align::Center);
    row.set_halign(gtk4::Align::Center);
    row.set_margin_start(SPACING);
    row.set_margin_end(SPACING);

    if config.util_updates {
        let (button, icon) = circle(theme, "deployed_code_update", 1.0);
        let tip = tooltip::Tooltip::new(&button, theme, tooltip::Kind::Styled);
        tooltip::hover_delay(&button, &tip, 0);
        let show = Rc::new({
            let button = button.clone();
            let updates = services.updates.clone();
            move || {
                button.set_visible(updates.advised());
                tip.set_text(&trf(
                    "%1 packages can be updated",
                    &[&updates.count.get().to_string()],
                ));
                text::set_color(
                    &icon,
                    if updates.strongly_advised() {
                        "colError"
                    } else {
                        "colOnLayer2"
                    },
                );
            }
        });
        show();
        scope.keep(services.updates.subscribe({
            let show = show.clone();
            move || show()
        }));
        scope.hold(watch::config("/updates", move || show()));
        button.connect_clicked(|_| {
            desktop::shell(&config::current().app_update);
        });
        row.append(&button);
    }

    if config.util_screen_snip {
        let (button, _) = circle(theme, "screenshot_region", 1.0);
        button.connect_clicked(|_| crate::core::actions::run("regionScreenshot"));
        row.append(&button);
    }

    if config.util_screen_record {
        let (button, _) = circle(theme, "videocam", 1.0);
        button.connect_clicked({
            let recording = services.recording.clone();
            move |_| {
                crate::core::process::launch_subcommand(&["record"]);
                recording.watch();
            }
        });
        row.append(&button);
    }

    if config.util_color_picker {
        let (button, _) = circle(theme, "colorize", 1.0);
        button.connect_clicked(|_| detach(&["hyprpicker", "-a"]));
        row.append(&button);
    }

    if config.util_keyboard {
        let (button, _) = circle(theme, "keyboard", 0.0);
        button.connect_clicked(|_| crate::core::actions::run("oskToggle"));
        row.append(&button);
    }

    if config.util_mic
        && let Some(audio) = &services.audio
    {
        let (button, icon) = circle(theme, "mic", 0.0);
        let show = {
            let audio = audio.clone();
            move || {
                icon.set_text(if audio.source_muted.get() {
                    "mic_off"
                } else {
                    "mic"
                });
            }
        };
        show();
        scope.keep(audio.subscribe(show));
        button.connect_clicked(|_| detach(&["wpctl", "set-mute", "@DEFAULT_SOURCE@", "toggle"]));
        row.append(&button);
    }

    if config.util_dark_mode {
        let (button, icon) = circle(theme, "dark_mode", 0.0);
        icon.set_text(if theme.borrow().m3.darkmode {
            "light_mode"
        } else {
            "dark_mode"
        });
        button.connect_clicked({
            let theme = theme.clone();
            move |_| {
                let mode = if theme.borrow().m3.darkmode {
                    "light"
                } else {
                    "dark"
                };
                crate::theming::switchwall::detach(&["--mode", mode, "--noswitch"]);
            }
        });
        scope.keep(services.session.subscribe({
            let theme = theme.clone();
            move || {
                icon.set_text(if theme.borrow().m3.darkmode {
                    "light_mode"
                } else {
                    "dark_mode"
                });
            }
        }));
        row.append(&button);
    }

    if config.util_power_profile {
        let (button, icon) = circle(theme, "airwave", 0.0);
        let show = {
            let power = services.power.clone();
            move || icon.set_text(profile_symbol(&power.profile.borrow()))
        };
        show();
        scope.keep(services.power.subscribe(show));
        button.connect_clicked({
            let power = services.power.clone();
            move |_| power.cycle()
        });
        row.append(&button);
    }

    row.upcast()
}

fn profile_symbol(profile: &str) -> &'static str {
    match profile {
        "power-saver" => "energy_savings_leaf",
        "performance" => "local_fire_department",
        _ => "airwave",
    }
}

fn circle(theme: &SharedTheme, name: &str, fill: f64) -> (RippleButton, gtk4::Label) {
    let icon = text::symbol_filled(name, pixel_size::LARGE as f64, fill);
    text::set_color(&icon, "colOnLayer2");
    icon.set_halign(gtk4::Align::Center);
    icon.set_valign(gtk4::Align::Center);

    let button = RippleButton::new(theme);
    button.set_size_request(SIZE, SIZE);
    button.set_valign(gtk4::Align::Center);
    button.set_child(Some(&icon));
    (button, icon)
}
