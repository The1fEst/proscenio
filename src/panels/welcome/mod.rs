mod defaults;
mod preference;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::i18n::{self, tr, trf};
use crate::core::{actions, config, paths, process, shell, watch};
use crate::panels::settings::arrangement::Arrangement;
use crate::panels::settings::content::{Choice, Page};
use crate::panels::settings::pages::desktop::displays::show_write_errors;
use crate::panels::settings::pages::desktop::power::{
    IdleTimeout, check_hypridle_available, idle_timeout_row,
};
use crate::panels::settings::pages::desktop::sound::device_label;
use crate::panels::settings::pages::quick::{self, bar_position, corner_style};
use crate::platform::dbus::notify::{self, Notification};
use crate::services::Services;
use crate::services::hardware::displays::{Displays, format_number, rates_of, shown_modes_of};
use crate::services::hyprland::idleoptions::IdleOptions;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::input::controls::{self, Switch};
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::flow::Flow;
use crate::ui::widgets::layout::group::ButtonGroup;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

const TITLE: &str = "proscenio Welcome";
const WIDTH: i32 = 900;
const HEIGHT: i32 = 650;
const MIN_WIDTH: i32 = 600;
const MIN_HEIGHT: i32 = 400;
const PADDING: i32 = 8;
const TITLE_START: i32 = 12;
const CLOSE_SIZE: i32 = 35;
const CLOSE_ICON: f64 = 20.0;
const SWITCH_SCALE: f64 = 0.6;
const CONTROLS_SPACING: i32 = 5;
const FLOW_SPACING: i32 = 5;
const NERD_BUTTON_HEIGHT: i32 = 35;
const NERD_BUTTON_PADDING: i32 = 10;
const NERD_BUTTON_SPACING: i32 = 5;
const FIRST_RUN_CONTENT: &str = "This file is just here to confirm you've been greeted :>";
const USAGE: &str = "https://the1fest.github.io/proscenio/docs/";
const CONFIGURATION: &str = "https://the1fest.github.io/proscenio/docs/settings";
const GITHUB: &str = "https://github.com/The1fEst/proscenio";
const NOTICE: &str = "Change any time later with /dark, /light, /wallpaper in the launcher\nIf the shell's colors aren't changing, open the sidebar and click \"Reload Hyprland & %1\" in its top-right corner";

pub struct Welcome {
    app: gtk4::Application,
    theme: SharedTheme,
    services: Rc<Services>,
    open_window: RefCell<Option<Shown>>,
}

struct Shown {
    window: gtk4::ApplicationWindow,
    _page: Rc<Page>,
    _owned_items: Vec<Box<dyn std::any::Any>>,
}

pub fn first_run_marker() -> PathBuf {
    paths::state().join("first_run.txt")
}

pub fn greet_if_first_run(welcome: &Rc<Welcome>) {
    let marker = first_run_marker();
    if marker.exists() {
        return;
    }
    let _ = std::fs::write(&marker, format!("{FIRST_RUN_CONTENT}\n"));
    let defaults_marker = paths::state().join("defaults_applied.txt");
    if !defaults_marker.exists() {
        let _ = std::fs::write(&defaults_marker, "");
        let displays = Displays::new();
        displays.set_primary(&displays.primary());
        defaults::apply();
    }
    welcome.open();
}

impl Welcome {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, services: &Rc<Services>) -> Rc<Self> {
        Rc::new(Welcome {
            app: app.clone(),
            theme: theme.clone(),
            services: services.clone(),
            open_window: RefCell::new(None),
        })
    }

    pub fn open(self: &Rc<Self>) {
        if let Some(open_window) = self.open_window.borrow().as_ref() {
            open_window.window.present();
            return;
        }
        let open_window = self.build();
        open_window.window.present();
        self.open_window.replace(Some(open_window));
    }

    pub fn close(&self) {
        if let Some(open_window) = self.open_window.take() {
            open_window.window.destroy();
        }
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.open_window.borrow().is_some() {
            self.close();
        } else {
            self.open();
        }
    }

    fn build(self: &Rc<Self>) -> Shown {
        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .title(TITLE)
            .default_width(WIDTH)
            .default_height(HEIGHT)
            .decorated(false)
            .build();
        window.set_size_request(MIN_WIDTH, MIN_HEIGHT);
        window.add_css_class("settings-window");

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, CONTROLS_SPACING);
        column.set_margin_top(PADDING);
        column.set_margin_bottom(PADDING);
        column.set_margin_start(PADDING);
        column.set_margin_end(PADDING);
        let mut owned_items: Vec<Box<dyn std::any::Any>> = Vec::new();
        let (titlebar, title_box, next_time_switch, tip) = self.titlebar();
        owned_items.push(Box::new(next_time_switch));
        owned_items.push(Box::new(tip));
        column.append(&titlebar);
        let arrange = move || {
            titlebar.set_visible(config::value_bool("/windows/showTitlebar", true));
            titlebar.set_start_widget(gtk4::Widget::NONE);
            titlebar.set_center_widget(gtk4::Widget::NONE);
            if config::value_bool("/windows/centerTitle", true) {
                title_box.set_margin_start(0);
                titlebar.set_center_widget(Some(&title_box));
            } else {
                title_box.set_margin_start(TITLE_START);
                titlebar.set_start_widget(Some(&title_box));
            }
        };
        arrange();
        owned_items.push(Box::new(watch::config("/windows", arrange)));

        let page = content(&self.theme, &self.services);
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.add_css_class("settings-content");
        container.set_overflow(gtk4::Overflow::Hidden);
        container.set_vexpand(true);
        container.append(&page.root);
        column.append(&container);
        window.set_child(Some(&column));

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let welcome = Rc::downgrade(self);
            move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                if let Some(welcome) = welcome.upgrade() {
                    welcome.close();
                }
                glib::Propagation::Stop
            }
        });
        window.add_controller(key_controller);
        window.connect_destroy({
            let welcome = Rc::downgrade(self);
            move |_| {
                if let Some(welcome) = welcome.upgrade() {
                    welcome.open_window.replace(None);
                }
                notify::send(&Notification {
                    app: "Shell",
                    summary: &tr("Welcome app"),
                    body: &tr("Enjoy! You can reopen the welcome app any time with <tt>Super+Shift+Alt+/</tt>. To open the settings app, hit <tt>Super+I</tt>"),
                    ..Default::default()
                });
            }
        });

        Shown {
            window,
            _page: page,
            _owned_items: owned_items,
        }
    }

    fn titlebar(self: &Rc<Self>) -> (gtk4::CenterBox, Centered, Rc<Switch>, Rc<Tooltip>) {
        let titlebar = gtk4::CenterBox::new();
        let title_label = gtk4::Label::new(Some(&tr("Hi there! First things first...")));
        text::set_font(
            &title_label,
            Family::Title,
            pixel_size::TITLE as f64,
            "wght=550",
        );
        text::set_color(&title_label, "colOnLayer0");
        let title_box = Centered::new(&title_label);
        title_box.set_vexpand(true);

        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, CONTROLS_SPACING);
        let next_time_label = text::styled_sized(&tr("Show next time"), pixel_size::SMALLER);
        controls.append(&Centered::new(&next_time_label));
        let next_time_switch = Switch::new(&self.theme);
        next_time_switch.set_scale(SWITCH_SCALE);
        next_time_switch.connect_clicked({
            let next_time_switch = Rc::downgrade(&next_time_switch);
            move || {
                let Some(next_time_switch) = next_time_switch.upgrade() else {
                    return;
                };
                let is_checked = !next_time_switch.is_checked();
                next_time_switch.set(is_checked);
                let marker = first_run_marker();
                if is_checked {
                    let _ = std::fs::remove_file(marker);
                } else {
                    let _ = std::fs::write(marker, format!("{FIRST_RUN_CONTENT}\n"));
                }
            }
        });
        controls.append(&next_time_switch.area);

        let close_button = RippleButton::new(&self.theme);
        close_button.set_radius(rounding::FULL as f64);
        close_button.set_size_request(CLOSE_SIZE, CLOSE_SIZE);
        close_button.set_valign(gtk4::Align::Center);
        let close_icon = text::symbol("close", CLOSE_ICON);
        close_button.set_content(&Centered::integral(&close_icon), 0, 0);
        close_button.connect_clicked({
            let welcome = Rc::downgrade(self);
            move |_| {
                if let Some(welcome) = welcome.upgrade() {
                    welcome.close();
                }
            }
        });
        let tip = Tooltip::new(&close_button, &self.theme, tooltip::Kind::Styled);
        tip.set_text(&tr("Tip: Close a window with Super+Q"));
        tooltip::hover_delay(&close_button, &tip, 0);
        controls.append(&close_button);
        titlebar.set_end_widget(Some(&controls));
        (titlebar, title_box, next_time_switch, tip)
    }
}

fn content(theme: &SharedTheme, services: &Rc<Services>) -> Rc<Page> {
    let page = Page::new(theme, true);
    language(&page);
    displays(&page, services);
    sound(&page, services);

    let bar_section = page.section("screenshot_monitor", &tr("Bar"));
    let row = page.row(&bar_section);
    let bar_position_section = page.subsection(&row, &tr("Bar position"), "");
    bar_position(&page, &bar_position_section);
    let bar_style_section = page.subsection(&row, &tr("Bar style"), "");
    corner_style(&page, &bar_style_section);

    let style_section = page.section("format_paint", &tr("Style & wallpaper"));
    let mode_group = ButtonGroup::new(&page.theme);
    mode_group.set_halign(gtk4::Align::Center);
    mode_group.append(&preference::build(&page, false));
    mode_group.append(&preference::build(&page, true));
    style_section.append(&mode_group);
    let choose_button = quick::choose_wallpaper(&page);
    choose_button.set_halign(gtk4::Align::Center);
    style_section.append(&choose_button);
    page.notice(&style_section, "info", &trf(NOTICE, &[shell::name()]));

    let power_section = page.section("bedtime", &tr("Power saving"));
    if check_hypridle_available(&page, &power_section) {
        let idle_options = IdleOptions::new();
        idle_timeout_row(
            &page,
            &power_section,
            &idle_options,
            &IdleTimeout {
                what: "screen",
                title: "Automatic Screen Blank",
                tip: "Turns the screens off after a period of inactivity",
                switch_icon: "brightness_low",
                switch_text: "Blank the screen",
                fallback_minutes: 15,
            },
        );
        idle_timeout_row(
            &page,
            &power_section,
            &idle_options,
            &IdleTimeout {
                what: "suspend",
                title: "Suspend when idle",
                tip: "Turning automatic suspend off means the machine keeps drawing power while nobody is at it",
                switch_icon: "pause",
                switch_text: "Suspend",
                fallback_minutes: 45,
            },
        );
        page.keep(idle_options);
    }

    let info_section = page.section("info", &tr("Info"));
    let links = Flow::new(FLOW_SPACING);
    let keybinds_button = quick::shortcut_button(
        &page,
        "keyboard_alt",
        &tr("Keybinds"),
        &[&quick::super_key()],
        "/",
    );
    keybinds_button.connect_clicked(|_| actions::run("cheatsheetToggle"));
    links.append(&keybinds_button);
    links.append(&link(&page, "help", &tr("Usage"), USAGE));
    links.append(&link(
        &page,
        "construction",
        &tr("Configuration"),
        CONFIGURATION,
    ));
    info_section.append(&links);

    let useless_section = page.section("monitoring", &tr("Useless buttons"));
    let buttons = Flow::new(FLOW_SPACING);
    buttons.append(&nerd_link(&page, "\u{f02a4}", &tr("GitHub"), GITHUB));
    useless_section.append(&buttons);
    page
}

fn language(page: &Rc<Page>) {
    let language_section = page.section("language", &tr("Language"));
    let select_section = page.subsection(&language_section, &tr("Select language"), "");
    let mut choices = vec![Choice {
        label: tr("Auto (System)"),
        icon: "",
        value: Value::from(i18n::AUTO),
    }];
    choices.extend(i18n::available().into_iter().map(|code| Choice {
        label: code.to_owned(),
        icon: "",
        value: Value::from(code),
    }));
    let language_setting = i18n::language_setting();
    page.selection(
        &select_section,
        choices,
        i18n::LANGUAGE,
        Value::from(i18n::AUTO),
        move |value| {
            let Some(code) = value.as_str().filter(|code| *code != language_setting) else {
                return;
            };
            i18n::choose(code, process::restart_shell_on_welcome);
        },
    );
}

fn displays(page: &Rc<Page>, services: &Rc<Services>) {
    struct State {
        displays: Rc<Displays>,
        selected_name: RefCell<String>,
    }
    impl State {
        fn monitor(&self) -> Option<crate::services::hardware::displays::Monitor> {
            let monitors = self.displays.monitors.borrow();
            let selected_name = self.selected_name.borrow();
            monitors
                .iter()
                .find(|monitor| monitor.name == *selected_name)
                .or(monitors.first())
                .cloned()
        }
    }

    let displays_section = page.section("display_settings", &tr("Displays"));
    let state = Rc::new(State {
        displays: Displays::new(),
        selected_name: RefCell::new(String::new()),
    });
    let choices_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    displays_section.append(&choices_box);
    let arrangement = Arrangement::new(&page.theme);
    displays_section.append(&arrangement.paint);
    show_write_errors(&state.displays, &displays_section);
    let row = page.row(&displays_section);
    let resolution_section = page.subsection(&row, &tr("Resolution"), "");
    let resolution_combo = page.combo(&resolution_section, "aspect_ratio");
    let rate_section = page.subsection(&row, &tr("Refresh rate"), "");
    let rate_combo = page.combo(&rate_section, "refresh");

    let selection: Rc<RefCell<Option<(Vec<String>, Rc<Selection>)>>> = Rc::default();
    let refresh: Rc<dyn Fn()> = Rc::new({
        let (state, arrangement, choices_box, resolution_combo, rate_combo, selection, theme) = (
            Rc::downgrade(&state),
            Rc::downgrade(&arrangement),
            choices_box.downgrade(),
            Rc::downgrade(&resolution_combo),
            Rc::downgrade(&rate_combo),
            selection.clone(),
            page.theme.clone(),
        );
        move || {
            let (
                Some(state),
                Some(arrangement),
                Some(choices_box),
                Some(resolution_combo),
                Some(rate_combo),
            ) = (
                state.upgrade(),
                arrangement.upgrade(),
                choices_box.upgrade(),
                resolution_combo.upgrade(),
                rate_combo.upgrade(),
            )
            else {
                return;
            };
            let monitors = state.displays.monitors.borrow().clone();
            let Some(monitor) = state.monitor() else {
                return;
            };
            let has_several_monitors = monitors.len() > 1;
            choices_box.set_visible(has_several_monitors);
            arrangement.paint.set_visible(has_several_monitors);
            arrangement.set(
                monitors
                    .iter()
                    .filter(|monitor| !monitor.is_disabled)
                    .cloned()
                    .collect(),
                &monitor.name,
            );
            let names: Vec<String> = monitors
                .iter()
                .map(|monitor| monitor.name.clone())
                .collect();
            let is_stale = selection
                .borrow()
                .as_ref()
                .is_none_or(|(selection_names, _)| *selection_names != names);
            if is_stale {
                if let Some((_, old_selection)) = selection.take() {
                    choices_box.remove(&old_selection.root);
                }
                let choices = monitors
                    .iter()
                    .map(|monitor| Choice {
                        label: format!(
                            "{} ({})",
                            if monitor.model.is_empty() {
                                &monitor.name
                            } else {
                                &monitor.model
                            },
                            monitor.name
                        ),
                        icon: "",
                        value: Value::from(monitor.name.clone()),
                    })
                    .collect();
                let state = Rc::downgrade(&state);
                let new_selection = Selection::new(&theme, choices, move |value| {
                    if let (Some(state), Some(name)) = (state.upgrade(), value.as_str()) {
                        state.selected_name.replace(name.to_owned());
                        state.displays.reload();
                    }
                });
                choices_box.append(&new_selection.root);
                selection.replace(Some((names, new_selection)));
            }
            if let Some((_, current_selection)) = selection.borrow().as_ref() {
                current_selection.set_current(&Value::from(monitor.name.clone()));
            }

            let modes = shown_modes_of(&monitor, false);
            let scale = monitor.scale.max(0.01);
            let logical_size = format!(
                "{}x{}",
                (monitor.width as f64 / scale).round(),
                (monitor.height as f64 / scale).round()
            );
            let values: Vec<String> = modes
                .iter()
                .map(|mode| format!("{}x{}", mode.width, mode.height))
                .collect();
            let labels: Vec<String> = modes
                .iter()
                .map(|mode| {
                    if mode.is_native {
                        trf(
                            "%1 × %2 (Default)",
                            &[&mode.width.to_string(), &mode.height.to_string()],
                        )
                    } else {
                        format!("{} × {}", mode.width, mode.height)
                    }
                })
                .collect();
            resolution_combo.set_items(&labels, index_of(&values, &logical_size));
            let rates: Vec<i64> = rates_of(&monitor)
                .iter()
                .map(|rate| rate.round() as i64)
                .collect();
            let rate_labels: Vec<String> = rates
                .iter()
                .map(|rate| trf("%1 Hz", &[&rate.to_string()]))
                .collect();
            rate_combo.set_items(
                &rate_labels,
                index_of(&rates, &(monitor.refresh_rate.round() as i64)),
            );
        }
    });
    state.displays.connect_changed({
        let refresh = refresh.clone();
        move || refresh()
    });
    refresh();

    arrangement.connect_picked({
        let state = Rc::downgrade(&state);
        move |name| {
            if let Some(state) = state.upgrade() {
                state.selected_name.replace(name.to_owned());
                state.displays.reload();
            }
        }
    });
    arrangement.connect_moved({
        let state = Rc::downgrade(&state);
        move |name, x, y| {
            if let Some(state) = state.upgrade() {
                state.displays.move_to(name, x, y);
            }
        }
    });
    resolution_combo.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            let Some(mode) = shown_modes_of(&monitor, false).into_iter().nth(index) else {
                return;
            };
            state.displays.apply(
                &monitor,
                &[
                    (
                        "size".to_owned(),
                        format!("{}x{}", mode.mode.width, mode.mode.height),
                    ),
                    ("scale".to_owned(), format_number(mode.scale)),
                    (
                        "rate".to_owned(),
                        format_number(mode.rate.unwrap_or(monitor.refresh_rate)),
                    ),
                ],
            );
        }
    });
    rate_combo.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            if let Some(rate) = rates_of(&monitor).get(index) {
                state.displays.apply(
                    &monitor,
                    &[("rate".to_owned(), format_number(rate.round()))],
                );
            }
        }
    });
    page.keep(services.events.subscribe({
        let state = Rc::downgrade(&state);
        move |event, _| {
            if !matches!(
                event,
                "monitoradded" | "monitoraddedv2" | "monitorremoved" | "monitorremovedv2"
            ) {
                return;
            }
            if let Some(state) = state.upgrade() {
                state.displays.reload();
            }
        }
    }));
    page.keep((
        state,
        arrangement,
        resolution_combo,
        rate_combo,
        selection,
        refresh,
    ));
}

fn sound(page: &Rc<Page>, services: &Rc<Services>) {
    let Some(audio) = services.audio.clone() else {
        return;
    };
    let sound_section = page.section("volume_up", &tr("Sound"));
    for (is_sink, title, icon) in [(true, "Output", "speaker"), (false, "Input", "mic")] {
        let group = page.subsection(&sound_section, &tr(title), "");
        let device_combo = page.combo(&group, icon);
        let names: Rc<RefCell<Vec<String>>> = Rc::default();
        device_combo.connect_activated({
            let (audio, names) = (audio.clone(), names.clone());
            move |index| {
                if let Some(name) = names.borrow().get(index) {
                    audio.set_default(is_sink, name);
                }
            }
        });
        let refresh = {
            let (audio, device_combo, names) =
                (audio.clone(), Rc::downgrade(&device_combo), names.clone());
            move || {
                let (device_combo, names) = (device_combo.clone(), names.clone());
                audio.read_devices(is_sink, move |devices, current_name| {
                    let Some(device_combo) = device_combo.upgrade() else {
                        return;
                    };
                    let labels: Vec<String> = devices.iter().map(device_label).collect();
                    let index = devices
                        .iter()
                        .position(|device| device.name == current_name)
                        .unwrap_or(0);
                    device_combo.set_items(&labels, index as i32);
                    names.replace(devices.into_iter().map(|device| device.name).collect());
                });
            }
        };
        refresh();
        page.keep(audio.subscribe(refresh));
    }
}

fn link(page: &Page, icon: &str, label: &str, url: &'static str) -> RippleButton {
    let (button, _) = controls::icon_button(&page.theme, icon, true, label);
    button.connect_clicked(move |_| open_url(url));
    button
}

fn nerd_link(page: &Page, glyph: &str, label: &str, url: &'static str) -> RippleButton {
    let button = RippleButton::new(&page.theme);
    button.set_radius(rounding::SMALL as f64);
    button.set_look(Look {
        background: |theme| theme.colors.col_layer2,
        ..Look::default()
    });
    button.set_size_request(-1, NERD_BUTTON_HEIGHT);
    button.set_valign(gtk4::Align::Center);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, NERD_BUTTON_SPACING);
    let glyph_label = gtk4::Label::new(Some(glyph));
    text::set_font(&glyph_label, Family::Nerd, pixel_size::LARGER as f64, "");
    text::set_color(&glyph_label, "colOnSecondaryContainer");
    row.append(&Centered::integral(&glyph_label));
    let name_label = text::styled(label);
    text::set_color(&name_label, "colOnSecondaryContainer");
    row.append(&Centered::new(&name_label));
    button.set_content(&row, NERD_BUTTON_PADDING, 0);
    button.connect_clicked(move |_| open_url(url));
    button
}

fn open_url(url: &str) {
    let _ = gio::AppInfo::launch_default_for_uri(url, gio::AppLaunchContext::NONE);
}

fn index_of<T: PartialEq>(values: &[T], target: &T) -> i32 {
    values
        .iter()
        .position(|value| value == target)
        .map_or(0, |index| index as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_welcome_links_lead_to_proscenio_pages() {
        for url in [USAGE, CONFIGURATION, GITHUB] {
            assert!(
                url.starts_with("https://the1fest.github.io/proscenio/")
                    || url == "https://github.com/The1fEst/proscenio",
                "{url}"
            );
        }
    }
}
