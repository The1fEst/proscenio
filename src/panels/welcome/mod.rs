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
use crate::core::{actions, config, paths, process, watch};
use crate::panels::settings::arrangement::Arrangement;
use crate::panels::settings::content::{Choice, Page};
use crate::panels::settings::pages::power::{IdleTimeout, hypridle_available, idle_timeout_row};
use crate::panels::settings::pages::quick::{self, bar_position, corner_style};
use crate::panels::settings::pages::sound::device_label;
use crate::platform::notify::{self, Notification};
use crate::services::Services;
use crate::services::displays::{Displays, number, rates_of, shown_modes_of};
use crate::services::idleoptions::IdleOptions;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::{self, Switch};
use crate::ui::widgets::flow::Flow;
use crate::ui::widgets::group::ButtonGroup;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::text::{self, Family};
use crate::ui::widgets::tooltip::{self, Tooltip};

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
const USAGE: &str = "https://end-4.github.io/dots-hyprland-wiki/en/ii-qs/02usage/";
const CONFIGURATION: &str = "https://end-4.github.io/dots-hyprland-wiki/en/ii-qs/03config/";
const GITHUB: &str = "https://github.com/end-4/dots-hyprland";
const SPONSORS: &str = "https://github.com/sponsors/end-4";
const NOTICE: &str = "Change any time later with /dark, /light, /wallpaper in the launcher\nIf the shell's colors aren't changing:\n    1. Open the right sidebar with Super+N\n    2. Click \"Reload Hyprland & Quickshell\" in the top-right corner";

pub struct Welcome {
    app: gtk4::Application,
    theme: SharedTheme,
    services: Rc<Services>,
    window: RefCell<Option<Shown>>,
}

struct Shown {
    window: gtk4::ApplicationWindow,
    _page: Rc<Page>,
    _held: Vec<Box<dyn std::any::Any>>,
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
    let applied = paths::state().join("defaults_applied.txt");
    if !applied.exists() {
        let _ = std::fs::write(&applied, "");
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
            window: RefCell::new(None),
        })
    }

    pub fn open(self: &Rc<Self>) {
        if let Some(shown) = self.window.borrow().as_ref() {
            shown.window.present();
            return;
        }
        let shown = self.build();
        shown.window.present();
        self.window.replace(Some(shown));
    }

    pub fn close(&self) {
        if let Some(shown) = self.window.take() {
            shown.window.destroy();
        }
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.window.borrow().is_some() {
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
        let mut held: Vec<Box<dyn std::any::Any>> = Vec::new();
        let (titlebar, title, switch, tip) = self.titlebar();
        held.push(Box::new(switch));
        held.push(Box::new(tip));
        column.append(&titlebar);
        let arrange = move || {
            titlebar.set_visible(config::value_bool("/windows/showTitlebar", true));
            titlebar.set_start_widget(gtk4::Widget::NONE);
            titlebar.set_center_widget(gtk4::Widget::NONE);
            if config::value_bool("/windows/centerTitle", true) {
                title.set_margin_start(0);
                titlebar.set_center_widget(Some(&title));
            } else {
                title.set_margin_start(TITLE_START);
                titlebar.set_start_widget(Some(&title));
            }
        };
        arrange();
        held.push(Box::new(watch::config("/windows", arrange)));

        let page = content(&self.theme, &self.services);
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.add_css_class("settings-content");
        container.set_overflow(gtk4::Overflow::Hidden);
        container.set_vexpand(true);
        container.append(&page.root);
        column.append(&container);
        window.set_child(Some(&column));

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
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
        window.add_controller(keys);
        window.connect_destroy({
            let welcome = Rc::downgrade(self);
            move |_| {
                if let Some(welcome) = welcome.upgrade() {
                    welcome.window.replace(None);
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
            _held: held,
        }
    }

    fn titlebar(self: &Rc<Self>) -> (gtk4::CenterBox, Centred, Rc<Switch>, Rc<Tooltip>) {
        let bar = gtk4::CenterBox::new();
        let title = gtk4::Label::new(Some(&tr("Hi there! First things first...")));
        text::set_font(&title, Family::Title, pixel_size::TITLE as f64, "wght=550");
        text::set_color(&title, "colOnLayer0");
        let placed = Centred::new(&title);
        placed.set_vexpand(true);

        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, CONTROLS_SPACING);
        let next_time = text::styled_sized(&tr("Show next time"), pixel_size::SMALLER);
        controls.append(&Centred::new(&next_time));
        let switch = Switch::new(&self.theme);
        switch.set_scale(SWITCH_SCALE);
        switch.connect_clicked({
            let switch = Rc::downgrade(&switch);
            move || {
                let Some(switch) = switch.upgrade() else {
                    return;
                };
                let checked = !switch.checked();
                switch.set(checked);
                let marker = first_run_marker();
                if checked {
                    let _ = std::fs::remove_file(marker);
                } else {
                    let _ = std::fs::write(marker, format!("{FIRST_RUN_CONTENT}\n"));
                }
            }
        });
        controls.append(&switch.area);

        let close = RippleButton::new(&self.theme);
        close.set_radius(rounding::FULL as f64);
        close.set_size_request(CLOSE_SIZE, CLOSE_SIZE);
        close.set_valign(gtk4::Align::Center);
        let icon = text::symbol("close", CLOSE_ICON);
        close.set_content(&Centred::integral(&icon), 0, 0);
        close.connect_clicked({
            let welcome = Rc::downgrade(self);
            move |_| {
                if let Some(welcome) = welcome.upgrade() {
                    welcome.close();
                }
            }
        });
        let tip = Tooltip::new(&close, &self.theme, tooltip::Kind::Styled);
        tip.set_text(&tr("Tip: Close a window with Super+Q"));
        tooltip::hover_delay(&close, &tip, 0);
        controls.append(&close);
        bar.set_end_widget(Some(&controls));
        (bar, placed, switch, tip)
    }
}

fn content(theme: &SharedTheme, services: &Rc<Services>) -> Rc<Page> {
    let page = Page::new(theme, true);
    language(&page);
    displays(&page, services);
    sound(&page, services);

    let bar = page.section("screenshot_monitor", &tr("Bar"));
    let row = page.row(&bar);
    let position = page.subsection(&row, &tr("Bar position"), "");
    bar_position(&page, &position);
    let style = page.subsection(&row, &tr("Bar style"), "");
    corner_style(&page, &style);

    let styling = page.section("format_paint", &tr("Style & wallpaper"));
    let modes = ButtonGroup::new(&page.theme);
    modes.set_halign(gtk4::Align::Center);
    modes.append(&preference::build(&page, false));
    modes.append(&preference::build(&page, true));
    styling.append(&modes);
    let choose = quick::choose_wallpaper(&page);
    choose.set_halign(gtk4::Align::Center);
    styling.append(&choose);
    page.notice(&styling, "info", &tr(NOTICE));

    let power = page.section("bedtime", &tr("Power saving"));
    if hypridle_available(&page, &power) {
        let options = IdleOptions::new();
        idle_timeout_row(
            &page,
            &power,
            &options,
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
            &power,
            &options,
            &IdleTimeout {
                what: "suspend",
                title: "Suspend when idle",
                tip: "Turning automatic suspend off means the machine keeps drawing power while nobody is at it",
                switch_icon: "pause",
                switch_text: "Suspend",
                fallback_minutes: 45,
            },
        );
        page.keep(options);
    }

    let info = page.section("info", &tr("Info"));
    let links = Flow::new(FLOW_SPACING);
    let keybinds = quick::shortcut_button(
        &page,
        "keyboard_alt",
        &tr("Keybinds"),
        &[&quick::super_key()],
        "/",
    );
    keybinds.connect_clicked(|_| actions::run("cheatsheetToggle"));
    links.append(&keybinds);
    links.append(&link(&page, "help", &tr("Usage"), USAGE));
    links.append(&link(
        &page,
        "construction",
        &tr("Configuration"),
        CONFIGURATION,
    ));
    info.append(&links);

    let useless = page.section("monitoring", &tr("Useless buttons"));
    let buttons = Flow::new(FLOW_SPACING);
    buttons.append(&nerd_link(&page, "\u{f02a4}", &tr("GitHub"), GITHUB));
    buttons.append(&link(&page, "favorite", &tr("Funny number"), SPONSORS));
    useless.append(&buttons);
    page
}

fn language(page: &Rc<Page>) {
    let section = page.section("language", &tr("Language"));
    let group = page.subsection(&section, &tr("Select language"), "");
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
    let chosen = i18n::chosen();
    page.selection(
        &group,
        choices,
        i18n::LANGUAGE,
        Value::from(i18n::AUTO),
        move |value| {
            if value.as_str() == Some(chosen.as_str()) {
                return;
            }
            config::store_value(i18n::LANGUAGE, value);
            process::restart_shell_on_welcome();
        },
    );
}

fn displays(page: &Rc<Page>, services: &Rc<Services>) {
    struct State {
        displays: Rc<Displays>,
        selected: RefCell<String>,
    }
    impl State {
        fn monitor(&self) -> Option<crate::services::displays::Monitor> {
            let monitors = self.displays.monitors.borrow();
            let selected = self.selected.borrow();
            monitors
                .iter()
                .find(|monitor| monitor.name == *selected)
                .or(monitors.first())
                .cloned()
        }
    }

    let section = page.section("display_settings", &tr("Displays"));
    let state = Rc::new(State {
        displays: Displays::new(),
        selected: RefCell::new(String::new()),
    });
    let choices = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    section.append(&choices);
    let arrangement = Arrangement::new(&page.theme);
    section.append(&arrangement.paint);
    let row = page.row(&section);
    let resolution_group = page.subsection(&row, &tr("Resolution"), "");
    let resolution = page.combo(&resolution_group, "aspect_ratio");
    let rate_group = page.subsection(&row, &tr("Refresh rate"), "");
    let rate = page.combo(&rate_group, "refresh");

    let selection: Rc<RefCell<Option<(Vec<String>, Rc<Selection>)>>> = Rc::default();
    let refresh: Rc<dyn Fn()> = Rc::new({
        let (state, arrangement, choices, resolution, rate, selection, theme) = (
            Rc::downgrade(&state),
            Rc::downgrade(&arrangement),
            choices.downgrade(),
            Rc::downgrade(&resolution),
            Rc::downgrade(&rate),
            selection.clone(),
            page.theme.clone(),
        );
        move || {
            let (Some(state), Some(arrangement), Some(choices), Some(resolution), Some(rate)) = (
                state.upgrade(),
                arrangement.upgrade(),
                choices.upgrade(),
                resolution.upgrade(),
                rate.upgrade(),
            ) else {
                return;
            };
            let monitors = state.displays.monitors.borrow().clone();
            let Some(monitor) = state.monitor() else {
                return;
            };
            let several = monitors.len() > 1;
            choices.set_visible(several);
            arrangement.paint.set_visible(several);
            arrangement.set(
                monitors
                    .iter()
                    .filter(|monitor| !monitor.disabled)
                    .cloned()
                    .collect(),
                &monitor.name,
            );
            let names: Vec<String> = monitors
                .iter()
                .map(|monitor| monitor.name.clone())
                .collect();
            let stale = selection
                .borrow()
                .as_ref()
                .is_none_or(|(shown, _)| *shown != names);
            if stale {
                if let Some((_, old)) = selection.take() {
                    choices.remove(&old.root);
                }
                let options = monitors
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
                let picked = Rc::downgrade(&state);
                let made = Selection::new(&theme, options, move |value| {
                    if let (Some(state), Some(name)) = (picked.upgrade(), value.as_str()) {
                        state.selected.replace(name.to_owned());
                        state.displays.reload();
                    }
                });
                choices.append(&made.root);
                selection.replace(Some((names, made)));
            }
            if let Some((_, made)) = selection.borrow().as_ref() {
                made.set_current(&Value::from(monitor.name.clone()));
            }

            let modes = shown_modes_of(&monitor, false);
            let scale = monitor.scale.max(0.01);
            let logical = format!(
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
                    if mode.native {
                        trf(
                            "%1 × %2 (Default)",
                            &[&mode.width.to_string(), &mode.height.to_string()],
                        )
                    } else {
                        format!("{} × {}", mode.width, mode.height)
                    }
                })
                .collect();
            resolution.set_items(&labels, index_of(&values, &logical));
            let rates: Vec<i64> = rates_of(&monitor)
                .iter()
                .map(|rate| rate.round() as i64)
                .collect();
            let rate_labels: Vec<String> = rates
                .iter()
                .map(|rate| trf("%1 Hz", &[&rate.to_string()]))
                .collect();
            rate.set_items(
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
                state.selected.replace(name.to_owned());
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
    resolution.connect_activated({
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
                    ("scale".to_owned(), number(mode.scale)),
                    (
                        "rate".to_owned(),
                        number(mode.rate.unwrap_or(monitor.refresh_rate)),
                    ),
                ],
            );
        }
    });
    rate.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            if let Some(rate) = rates_of(&monitor).get(index) {
                state
                    .displays
                    .apply(&monitor, &[("rate".to_owned(), number(rate.round()))]);
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
    page.keep((state, arrangement, resolution, rate, selection, refresh));
}

fn sound(page: &Rc<Page>, services: &Rc<Services>) {
    let Some(audio) = services.audio.clone() else {
        return;
    };
    let section = page.section("volume_up", &tr("Sound"));
    for (sink, title, icon) in [(true, "Output", "speaker"), (false, "Input", "mic")] {
        let group = page.subsection(&section, &tr(title), "");
        let combo = page.combo(&group, icon);
        let names: Rc<RefCell<Vec<String>>> = Rc::default();
        combo.connect_activated({
            let (audio, names) = (audio.clone(), names.clone());
            move |index| {
                if let Some(name) = names.borrow().get(index) {
                    audio.set_default(sink, name);
                }
            }
        });
        let refresh = {
            let (audio_ref, combo, names) = (audio.clone(), Rc::downgrade(&combo), names.clone());
            move || {
                let (combo, names) = (combo.clone(), names.clone());
                audio_ref.devices(sink, move |devices, current| {
                    let Some(combo) = combo.upgrade() else {
                        return;
                    };
                    let labels: Vec<String> = devices.iter().map(device_label).collect();
                    let index = devices
                        .iter()
                        .position(|device| device.name == current)
                        .unwrap_or(0);
                    combo.set_items(&labels, index as i32);
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
    let icon = gtk4::Label::new(Some(glyph));
    text::set_font(&icon, Family::Nerd, pixel_size::LARGER as f64, "");
    text::set_color(&icon, "colOnSecondaryContainer");
    row.append(&Centred::integral(&icon));
    let name = text::styled(label);
    text::set_color(&name, "colOnSecondaryContainer");
    row.append(&Centred::new(&name));
    button.set_content(&row, NERD_BUTTON_PADDING, 0);
    button.connect_clicked(move |_| open_url(url));
    button
}

fn open_url(url: &str) {
    let _ = gio::AppInfo::launch_default_for_uri(url, gio::AppLaunchContext::NONE);
}

fn index_of<T: PartialEq>(values: &[T], current: &T) -> i32 {
    values
        .iter()
        .position(|value| value == current)
        .map_or(0, |index| index as i32)
}
