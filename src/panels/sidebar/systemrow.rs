use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::core::process::detach;
use crate::core::scope::Scope;
use crate::core::shell;
use crate::panels::sessionscreen::SessionScreen;
use crate::panels::settings::Settings;
use crate::services::background::BackgroundTasks;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::group::{ButtonGroup, GroupButton};
use crate::ui::widgets::row::Row;
use crate::ui::widgets::{customicon, text, tooltip};

const ICON: i32 = 25;
const BUTTON: f64 = 40.0;
const SYMBOL: f64 = 22.0;

pub struct Actions {
    pub close: Rc<dyn Fn()>,
    pub edit: Option<Rc<dyn Fn(bool)>>,
    pub settings: Rc<Settings>,
}

pub fn build(
    theme: &SharedTheme,
    background: &Rc<BackgroundTasks>,
    actions: Actions,
    session: &Rc<SessionScreen>,
    scope: &Scope,
) -> (gtk4::Widget, Option<Rc<dyn Fn()>>) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.set_margin_top(5);
    row.append(&uptime_pill(background, scope));

    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    row.append(&spacer);

    let group = ButtonGroup::new(theme);
    group.set_colour(|theme| theme.colors.col_layer1);
    group.set_padding(4.0);

    let mut stop_editing: Option<Rc<dyn Fn()>> = None;
    if let Some(edit) = actions.edit {
        let (button, tip, set_toggled) = quick_toggle(theme, "edit", &tr("Edit quick toggles"));
        let editing = Rc::new(Cell::new(false));
        let set_editing = {
            let editing = editing.clone();
            move |now: bool| {
                editing.set(now);
                set_toggled(now);
                let mut text = tr("Edit quick toggles");
                if now {
                    text.push('\n');
                    text.push_str(&tr("Drag to move, add and remove\nRMB to toggle size"));
                }
                tip.set_text(&text);
                edit(now);
            }
        };
        let set_editing = Rc::new(set_editing);
        button.connect_clicked({
            let editing = editing.clone();
            let set_editing = set_editing.clone();
            move || set_editing(!editing.get())
        });
        stop_editing = Some(Rc::new(move || {
            if editing.get() {
                set_editing(false);
            }
        }));
        group.append(&button);
    }

    let (reload, _, _) = quick_toggle(
        theme,
        "restart_alt",
        &trf("Reload Hyprland & %1", &[shell::name()]),
    );
    reload.connect_clicked(|| {
        detach(&["hyprctl", "reload"]);
        crate::core::process::restart_shell();
    });
    group.append(&reload);

    let (settings, _, _) = quick_toggle(theme, "settings", &tr("Settings"));
    let close = actions.close;
    let window = actions.settings;
    settings.connect_clicked(move || {
        close();
        window.open(None);
    });
    group.append(&settings);

    let (power, _, _) = quick_toggle(theme, "power_settings_new", &tr("Session"));
    power.connect_clicked({
        let session = session.clone();
        move || session.open()
    });
    group.append(&power);

    row.append(&group);
    (row.upcast(), stop_editing)
}

fn quick_toggle(
    theme: &SharedTheme,
    icon: &str,
    tip: &str,
) -> (GroupButton, Rc<tooltip::Tooltip>, Rc<dyn Fn(bool)>) {
    let button = GroupButton::new(theme, BUTTON, BUTTON);
    button.set_clicked_width(BUTTON + 20.0);
    button.set_radii(BUTTON / 2.0, rounding::SMALL as f64);
    button.jump_radius();

    let symbol = text::symbol(icon, SYMBOL);
    symbol.add_css_class("color-fade");
    text::set_color(&symbol, "colOnLayer1");
    let fill = text::fill_motion(&symbol, SYMBOL, 0.0);
    button.set_content(&Centred::new(&symbol));

    let tooltip = tooltip::Tooltip::new(&button, theme, tooltip::Kind::Styled);
    tooltip.place_like_qt();
    tooltip.set_text(tip);
    tooltip::hover_delay(&button, &tooltip, 0);

    let set_toggled: Rc<dyn Fn(bool)> = {
        let button = button.downgrade();
        Rc::new(move |toggled| {
            let Some(button) = button.upgrade() else {
                return;
            };
            button.set_toggled(toggled);
            fill(if toggled { 1.0 } else { 0.0 });
            text::set_color(
                &symbol,
                if toggled {
                    "m3onPrimary"
                } else {
                    "colOnLayer1"
                },
            );
        })
    };
    (button, tooltip, set_toggled)
}

fn uptime_pill(background: &Rc<BackgroundTasks>, scope: &Scope) -> gtk4::Widget {
    let icon = customicon::build(&format!("{}-symbolic", family()), ICON);
    icon.set_valign(gtk4::Align::Center);
    text::set_color(&icon, "colOnLayer0");

    let label = text::styled_sized("", pixel_size::NORMAL);
    text::set_color(&label, "colOnLayer0");

    let inside = Row::new(8);
    inside.append(&icon);
    inside.append(&label);

    let pill = Centred::integral(&inside);
    pill.add_css_class("uptime-pill");

    let tick = move || label.set_text(&trf("Up %1", &[&uptime()]));
    tick();
    scope.keep(background.add_scoped("uptime", move || {
        tick();
        Ok(())
    }));

    pill.upcast()
}

fn uptime() -> String {
    let seconds = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;
    let (days, hours, minutes) = (
        seconds / 86_400,
        seconds % 86_400 / 3_600,
        seconds % 3_600 / 60,
    );

    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 || parts.is_empty() {
        parts.push(format!("{minutes}m"));
    }
    parts.join(", ")
}

fn family() -> &'static str {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    crate::services::sysinfo::distro_family(&release)
}
