use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::config::{self, Config};
use crate::core::i18n::tr;
use crate::panels::settings::Settings;
use crate::panels::settings::pages::connectivity::network::connection::NEW_ENTERPRISE;
use crate::panels::sidebar::toggles::Menu;
use crate::panels::wifinetwork::{self, NetworkList};
use crate::services::Services;
use crate::services::network::net;
use crate::services::network::wifi::Wifi;
use crate::services::session::parse_clock_time;
use crate::ui::theme::{SharedTheme, pixel_size, transparentize};
use crate::ui::widgets::display::customicon;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::ConfigSwitch;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::input::slider::{Options, SMALL, Slider};
use crate::ui::widgets::input::spinbox::SpinBox;
use crate::ui::widgets::input::textfield::{Style, TextField};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::column::Column;
use crate::ui::widgets::overlay::windowdialog::{
    PADDING, Place, WindowDialog, button, button_row, list_item, progress, section_header,
    separator, separator_place, spacer, title,
};

const LIST_TOP: f64 = -15.0;
const LIST_BOTTOM: f64 = -16.0;
const ITEM_VERTICAL: i32 = 12;
const ICON: f64 = pixel_size::LARGER as f64;

pub struct Context {
    pub services: Rc<Services>,
    pub theme: SharedTheme,
    pub close_sidebar: Rc<dyn Fn()>,
    pub settings: Rc<Settings>,
    pub screen: String,
}

pub fn open(menu: Menu, context: &Context) -> Rc<WindowDialog> {
    match menu {
        Menu::Wifi => wifi(context),
        Menu::Bluetooth => bluetooth(context),
        Menu::NightLight => night(context),
        Menu::WireGuard => wireguard(context),
    }
}

fn list_place() -> Place {
    Place {
        top: LIST_TOP,
        bottom: LIST_BOTTOM,
        left: -PADDING,
        right: -PADDING,
        fills_width: true,
        fills_height: true,
    }
}

fn scroller(child: &impl IsA<gtk4::Widget>) -> gtk4::ScrolledWindow {
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_child(Some(child));
    crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);
    scroll
}

fn done_button(context: &Context, dialog: &Rc<WindowDialog>) -> RippleButton {
    let dismiss_button = button(&context.theme, &tr("Done"));
    let weak = Rc::downgrade(dialog);
    dismiss_button.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.dismiss();
        }
    });
    dismiss_button
}

fn footer(
    context: &Context,
    dialog: &Rc<WindowDialog>,
    details: &'static str,
    page: impl Fn() -> &'static str + 'static,
) {
    let (row, place) = button_row();
    let details_button = button(&context.theme, &tr(details));
    let close = context.close_sidebar.clone();
    let settings = context.settings.clone();
    details_button.connect_clicked(move |_| {
        close();
        settings.open(Some(page()));
    });
    row.append(&details_button);
    row.append(&spacer());
    row.append(&done_button(context, dialog));
    dialog.column.add(&row, place);
}

fn tint(label: &gtk4::Label, token: &str) -> gtk4::Label {
    text::set_color(label, token);
    label.clone()
}

fn elide(label: gtk4::Label) -> gtk4::Label {
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_xalign(0.0);
    label
}

fn wifi(context: &Context) -> Rc<WindowDialog> {
    let theme = &context.theme;
    let dialog = WindowDialog::new(theme, Some(600.0));
    dialog
        .column
        .add(&title(&tr("Connect to Wi-Fi")), Place::default());
    let rule = separator();
    rule.set_visible(false);
    dialog.column.add(&rule, separator_place());
    let bar = progress(theme);
    dialog.column.add(&bar, Place::bleed(-8.0, -8.0));

    let wifi = Wifi::new();
    let close = context.close_sidebar.clone();
    let settings = context.settings.clone();
    let list = NetworkList::new(
        theme,
        &wifi,
        wifinetwork::Options {
            shows_actions: false,
            heights: None,
        },
        move |ssid| {
            close();
            settings.open_with(Some("connection"), Some(&format!("{NEW_ENTERPRISE}{ssid}")));
        },
    );
    dialog.column.add(&scroller(&list.root), list_place());
    dialog.column.add(&separator(), separator_place());
    let symbol = context.services.net.symbol.clone();
    footer(context, &dialog, "Details", move || {
        if symbol.borrow().as_str() == "lan" {
            "network"
        } else {
            "wifi"
        }
    });

    let show_scan_state = {
        let wifi = Rc::downgrade(&wifi);
        move || {
            if let Some(wifi) = wifi.upgrade() {
                let is_scanning = wifi.state.borrow().is_scanning;
                rule.set_visible(!is_scanning);
                bar.set_visible(is_scanning);
            }
        }
    };
    wifi.connect_changed(show_scan_state);
    wifi.set_enabled(true);
    wifi.rescan();
    dialog.keep(wifi);
    dialog.keep(list);
    dialog
}

fn device_symbol(icon: &str) -> &'static str {
    if icon.contains("headset") || icon.contains("headphones") {
        "headphones"
    } else if icon.contains("audio") {
        "speaker"
    } else if icon.contains("phone") {
        "smartphone"
    } else if icon.contains("mouse") {
        "mouse"
    } else if icon.contains("keyboard") {
        "keyboard"
    } else {
        "bluetooth"
    }
}

fn action_button(theme: &SharedTheme, label: &str, look: Look, color: &str) -> RippleButton {
    let ripple_button = button(theme, label);
    ripple_button.set_look(look);
    if let Some(text) = ripple_button
        .child()
        .and_then(|holder| holder.first_child())
        .and_downcast::<gtk4::Label>()
    {
        text::set_color(&text, color);
    }
    ripple_button
}

fn bluetooth(context: &Context) -> Rc<WindowDialog> {
    let theme = &context.theme;
    let bluez = context.services.bluez.clone();
    let dialog = WindowDialog::new(theme, Some(600.0));
    dialog
        .column
        .add(&title(&tr("Bluetooth devices")), Place::default());
    let rule = separator();
    dialog.column.add(&rule, separator_place());
    let bar = progress(theme);
    dialog.column.add(&bar, Place::bleed(-8.0, -8.0));
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    dialog.column.add(&scroller(&list), list_place());
    dialog.column.add(&separator(), separator_place());
    footer(context, &dialog, "Details", || "bluetooth");

    let expanded_paths: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let fill: Rc<dyn Fn()> = Rc::new({
        let theme = theme.clone();
        let bluez = bluez.clone();
        let list = list.downgrade();
        let expanded_paths = expanded_paths.clone();
        move || {
            let Some(list) = list.upgrade() else {
                return;
            };
            let is_discovering = bluez.is_discovering.get();
            rule.set_visible(!is_discovering);
            bar.set_visible(is_discovering);
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            for device in bluez.devices.borrow().iter() {
                list.append(&bluetooth_item(&theme, &bluez, device, &expanded_paths));
            }
        }
    });
    fill();
    let weak = Rc::downgrade(&fill);
    dialog.keep(Rc::new(bluez.subscribe(move || {
        if let Some(fill) = weak.upgrade() {
            fill();
        }
    })));
    dialog.keep(Rc::new(fill));
    bluez.set_powered(true);
    bluez.set_discovering(true);
    dialog.connect_closed(move || bluez.set_discovering(false));
    dialog
}

fn bluetooth_item(
    theme: &SharedTheme,
    bluez: &crate::services::network::bluez::Bluez,
    device: &crate::services::network::bluez::Device,
    expanded_paths: &Rc<RefCell<Vec<String>>>,
) -> gtk4::Widget {
    let is_expanded = expanded_paths.borrow().contains(&device.path);
    let item = list_item(theme, false);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    row.append(&tint(
        &text::symbol(device_symbol(&device.icon), ICON),
        "colOnSurfaceVariant",
    ));
    let names = Column::filling_width(2);
    names.append(&elide(tint(
        &text::styled(&if device.name.is_empty() {
            tr("Unknown device")
        } else {
            device.name.clone()
        }),
        "colOnSurfaceVariant",
    )));
    if device.is_paired {
        let mut status = tr(if device.is_connected {
            "Connected"
        } else {
            "Paired"
        });
        if let Some(battery) = device.battery {
            status.push_str(&format!(" • {}%", (battery * 100.0).round()));
        }
        names.append(&elide(tint(
            &text::styled_sized(&status, pixel_size::SMALLER),
            "colSubtext",
        )));
    }
    let middle = Centered::filling_width(&names);
    middle.set_hexpand(true);
    row.append(&middle);
    let chevron = Centered::new(&tint(
        &text::symbol("keyboard_arrow_down", ICON),
        "colOnLayer3",
    ));
    chevron.set_rotation(if is_expanded { 180.0 } else { 0.0 });
    row.append(&chevron);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&row);
    if is_expanded {
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        actions.set_margin_top(8);
        actions.append(&spacer());
        let is_paired = device.is_paired;
        let first = action_button(
            theme,
            &tr(if is_paired {
                "Forget"
            } else {
                "Always connect"
            }),
            if is_paired {
                Look {
                    background: |theme| theme.colors.col_error,
                    hover: |theme| theme.colors.col_error_hover,
                    ripple: |theme| theme.colors.col_error_active,
                    ..Look::default()
                }
            } else {
                Look {
                    background: |theme| transparentize(theme.colors.col_layer3, 1.0),
                    hover: |theme| transparentize(theme.colors.col_layer3, 1.0),
                    ripple: |theme| theme.colors.col_layer3_hover,
                    ..Look::default()
                }
            },
            if is_paired {
                "colOnError"
            } else {
                "colPrimary"
            },
        );
        let path = device.path.clone();
        first.connect_clicked({
            let bluez = bluez.clone();
            let path = path.clone();
            move |_| {
                if is_paired {
                    bluez.forget(&path);
                } else {
                    bluez.pair(&path);
                }
            }
        });
        actions.append(&first);
        let is_connected = device.is_connected;
        let second = action_button(
            theme,
            &tr(if is_connected {
                "Disconnect"
            } else {
                "Connect"
            }),
            Look {
                background: |theme| theme.colors.col_primary,
                hover: |theme| theme.colors.col_primary_hover,
                ripple: |theme| theme.colors.col_primary_active,
                ..Look::default()
            },
            "colOnPrimary",
        );
        second.connect_clicked({
            let bluez = bluez.clone();
            move |_| bluez.set_device_connected(&path, !is_connected)
        });
        actions.append(&second);
        column.append(&actions);
    }
    item.set_content(&column, PADDING as i32, ITEM_VERTICAL);
    if is_expanded {
        item.set_cursor_from_name(None);
    }

    let flip: Rc<dyn Fn()> = {
        let expanded_paths = expanded_paths.clone();
        let path = device.path.clone();
        let bluez = bluez.clone();
        Rc::new(move || {
            {
                let mut paths = expanded_paths.borrow_mut();
                match paths.iter().position(|entry| *entry == path) {
                    Some(index) => {
                        paths.remove(index);
                    }
                    None => paths.push(path.clone()),
                }
            }
            bluez.refresh();
        })
    };
    item.connect_clicked({
        let flip = flip.clone();
        move |_| flip()
    });
    item.connect_alt(move |_| flip());
    item.upcast()
}

fn small_slider(theme: &SharedTheme, from: f64, to: f64) -> Rc<Slider> {
    Slider::with(
        theme,
        Options {
            track: SMALL,
            from,
            to,
            icon: None,
            secondary: None,
            dividers: Vec::new(),
        },
    )
}

fn dialog_slider(
    theme: &SharedTheme,
    label: Option<&str>,
    from: f64,
    to: f64,
) -> (gtk4::Widget, Rc<Slider>) {
    let column = Column::filling_width(-2);
    if let Some(label) = label {
        let name = tint(&text::styled(&tr(label)), "colSubtext");
        name.set_xalign(0.0);
        name.set_margin_start(2);
        column.append(&name);
    }
    let slider = small_slider(theme, from, to);
    slider.set_stops(vec![to]);
    slider.area.set_margin_start(4);
    slider.area.set_margin_end(4);
    column.append(&slider.area);
    column.set_margin_start(4);
    column.set_margin_end(4);
    (column.upcast(), slider)
}

fn build_clock_field(
    theme: &SharedTheme,
    label: &str,
    key: &'static str,
    current: fn(&Config) -> String,
) -> Rc<TextField> {
    let field = TextField::new(theme, Style::Outlined, label);
    field.connect_finished(move |text| {
        if let Some(time) = parse_clock_time(&text) {
            Config::store_night(key, serde_json::Value::from(time));
        }
    });
    field.bind(move || current(&config::current()));
    field
}

fn night(context: &Context) -> Rc<WindowDialog> {
    let theme = &context.theme;
    let session = context.services.session.clone();
    let light = context.services.light.clone();
    let dialog = WindowDialog::new(theme, Some(760.0));
    let column = &dialog.column;
    let header_rule = || Place {
        top: -22.0,
        bottom: -8.0,
        fills_width: true,
        ..Place::default()
    };
    let section = Place {
        top: -16.0,
        fills_width: true,
        ..Place::default()
    };

    column.add(&title(&tr("Eye protection")), Place::default());
    column.add(&section_header(&tr("Night Light")), Place::default());
    column.add(&separator(), header_rule());

    let group = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let now_switch = ConfigSwitch::new(theme, "check", &tr("Enable now"), {
        let session = session.clone();
        move |is_on| session.toggle_temperature(Some(is_on))
    });
    let automatic_switch = ConfigSwitch::new(theme, "night_sight_auto", &tr("Automatic"), {
        let session = session.clone();
        move |is_on| session.set_night_light_automatic(is_on)
    });
    group.append(&now_switch.button);
    group.append(&automatic_switch.button);

    let time_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    time_row.set_homogeneous(true);
    time_row.set_margin_start(8);
    time_row.set_margin_end(8);
    time_row.set_margin_top(4);
    time_row.set_margin_bottom(4);
    let from_field = build_clock_field(theme, &tr("From (HH:mm)"), "from", |config| {
        config.night_from.clone()
    });
    let to_field = build_clock_field(theme, &tr("To (HH:mm)"), "to", |config| {
        config.night_to.clone()
    });
    time_row.append(&from_field.root);
    time_row.append(&to_field.root);
    group.append(&time_row);

    let transition_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    transition_row.set_margin_start(8);
    transition_row.set_margin_end(8);
    transition_row.set_margin_top(4);
    transition_row.set_margin_bottom(4);
    let transition_icon = text::symbol("line_curve", pixel_size::LARGER as f64);
    text::set_color(&transition_icon, "colOnSecondaryContainer");
    transition_row.append(&Centered::integral(&transition_icon));
    let transition_name = text::styled(&tr("Transition (min)"));
    text::set_application_font(&transition_name, pixel_size::SMALL as f64);
    text::set_color(&transition_name, "colOnSecondaryContainer");
    transition_name.set_xalign(0.0);
    let transition_label = Centered::filling_width(&transition_name);
    transition_label.set_hexpand(true);
    transition_row.append(&transition_label);
    let transition_spin = SpinBox::new(theme, 0, 120, 5, 0);
    transition_spin.connect_changed(|minutes| {
        Config::store_night("transition", serde_json::Value::from(minutes));
    });
    transition_row.append(&transition_spin.root);
    group.append(&transition_row);

    let (intensity, strength) = dialog_slider(theme, Some("Intensity"), 6500.0, 1000.0);
    strength.set_stops(vec![5000.0, 1000.0]);
    strength.on_moved({
        let session = session.clone();
        let strength = Rc::downgrade(&strength);
        move |value| {
            session.set_temperature(value.round() as i32);
            if let Some(strength) = strength.upgrade() {
                strength.set_tooltip(&format!("{}K", value.round()));
            }
        }
    });
    group.append(&intensity);
    column.add(&group, section);

    column.add(&section_header(&tr("Brightness")), Place::default());
    column.add(&separator(), header_rule());
    let (level, level_slider) = dialog_slider(theme, None, 0.0, 1.0);
    level_slider.on_moved({
        let light = light.clone();
        let screen = context.screen.clone();
        move |value| light.set_level(&screen, value)
    });
    column.add(&level, section);

    column.add(&section_header(&tr("Gamma")), Place::default());
    column.add(&separator(), header_rule());
    let floor = crate::services::hardware::brightness::GAMMA_FLOOR / 100.0;
    let (gamma, gamma_slider) = dialog_slider(theme, None, floor, 1.0);
    gamma_slider.on_moved({
        let light = light.clone();
        let gamma_slider = Rc::downgrade(&gamma_slider);
        move |value| {
            light.set_gamma(value * 100.0);
            if let Some(slider) = gamma_slider.upgrade() {
                slider.set_tooltip(&format!("{}%", (value * 100.0).round()));
            }
        }
    });
    column.add(
        &gamma,
        Place {
            fills_height: true,
            ..section
        },
    );

    footer(context, &dialog, "Details", || "nightlight");

    let sync: Rc<dyn Fn()> = Rc::new({
        let session = session.clone();
        let light = light.clone();
        let screen = context.screen.clone();
        move || {
            now_switch.set(session.is_night_light_on.get());
            let is_automatic = session.is_night_light_automatic.get();
            automatic_switch.set(is_automatic);
            from_field.set_enabled(is_automatic);
            to_field.set_enabled(is_automatic);
            from_field.refresh();
            to_field.refresh();
            transition_spin.set_value(i64::from(config::current().night_transition));
            let kelvin = session.temperature.get() as f64;
            strength.set(kelvin);
            strength.set_tooltip(&format!("{}K", kelvin.round()));
            level_slider.set(light.level(&screen));
            gamma_slider.set(light.gamma.get() / 100.0);
            gamma_slider.set_tooltip(&format!("{}%", light.gamma.get().round()));
        }
    });
    sync();
    let weak = Rc::downgrade(&sync);
    dialog.keep(Rc::new(session.subscribe({
        let weak = weak.clone();
        move || {
            if let Some(sync) = weak.upgrade() {
                sync();
            }
        }
    })));
    dialog.keep(Rc::new(light.subscribe(move |_| {
        if let Some(sync) = weak.upgrade() {
            sync();
        }
    })));
    dialog.keep(Rc::new(sync));
    dialog
}

fn wireguard(context: &Context) -> Rc<WindowDialog> {
    let theme = &context.theme;
    let dialog = WindowDialog::new(theme, Some(400.0));
    dialog
        .column
        .add(&title(&tr("WireGuard Connections")), Place::default());
    dialog.column.add(&separator(), separator_place());
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    dialog.column.add(&scroller(&list), list_place());
    dialog.column.add(&separator(), separator_place());
    footer(context, &dialog, "New Connection", || "network");

    let refresh_slot: Rc<RefCell<Option<Weak<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let fill: Rc<dyn Fn()> = {
        let theme = theme.clone();
        let list = list.downgrade();
        let refresh_slot = refresh_slot.clone();
        Rc::new(move || {
            let theme = theme.clone();
            let list = list.clone();
            let refresh_slot = refresh_slot.clone();
            net::read_tunnels(move |tunnels| {
                let Some(list) = list.upgrade() else {
                    return;
                };
                while let Some(child) = list.first_child() {
                    list.remove(&child);
                }
                for (name, is_up) in tunnels {
                    list.append(&wireguard_item(&theme, &name, is_up, &refresh_slot));
                }
            });
        })
    };
    refresh_slot.replace(Some(Rc::downgrade(&fill)));
    fill();
    dialog.keep(Rc::new(fill));
    dialog
}

fn wireguard_item(
    theme: &SharedTheme,
    name: &str,
    is_up: bool,
    refresh_slot: &Rc<RefCell<Option<Weak<dyn Fn()>>>>,
) -> gtk4::Widget {
    let item = list_item(theme, is_up);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let icon = customicon::build("wireguard-symbolic", ICON as i32);
    text::set_color(&icon, "colOnSurfaceVariant");
    icon.set_valign(gtk4::Align::Center);
    row.append(&icon);
    let names = Column::filling_width(2);
    names.append(&elide(tint(&text::styled(name), "colOnSurfaceVariant")));
    names.append(&elide(tint(
        &text::styled_sized(
            &tr(if is_up { "Connected" } else { "Disconnected" }),
            pixel_size::SMALLER,
        ),
        "colSubtext",
    )));
    let middle = Centered::filling_width(&names);
    middle.set_hexpand(true);
    row.append(&middle);
    if is_up {
        row.append(&tint(&text::symbol("check", ICON), "colOnSurfaceVariant"));
    }
    item.set_content(&row, PADDING as i32, ITEM_VERTICAL);
    let name = name.to_owned();
    let refresh_slot = refresh_slot.clone();
    item.connect_clicked(move |_| {
        net::set_tunnel_up(&name, !is_up);
        let refresh_slot = refresh_slot.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
            let action = refresh_slot.borrow().as_ref().and_then(Weak::upgrade);
            if let Some(action) = action {
                action();
            }
        });
    });
    item.upcast()
}
