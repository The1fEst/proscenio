use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::notifications::list::Placeholder;
use crate::panels::settings::Settings;
use crate::panels::sidebar::toggles::Menu;
use crate::panels::wifinetwork::{self, NetworkList};
use crate::services::Services;
use crate::services::net;
use crate::services::wifi::Wifi;
use crate::ui::shapes::Shape;
use crate::ui::theme::{SharedTheme, pixel_size, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::column::Column;
use crate::ui::widgets::controls::{ComboBox, ConfigSwitch};
use crate::ui::widgets::customicon;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::slider::{Options, SMALL, Slider};
use crate::ui::widgets::text;
use crate::ui::widgets::tooltip::{self, Tooltip};
use crate::ui::widgets::windowdialog::{
    DialogColumn, PADDING, Place, WindowDialog, button, button_row, list_item, progress,
    section_header, separator, separator_place, spacer, title,
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
        Menu::AudioOut => volume(context, true),
        Menu::AudioIn => volume(context, false),
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
        fill_width: true,
        fill_height: true,
    }
}

fn scroller(child: &impl IsA<gtk4::Widget>) -> gtk4::ScrolledWindow {
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_child(Some(child));
    crate::ui::widgets::flickable::follow_scroll_settings(&scroll);
    scroll
}

fn done_button(context: &Context, dialog: &Rc<WindowDialog>) -> RippleButton {
    let done = button(&context.theme, &tr("Done"));
    let weak = Rc::downgrade(dialog);
    done.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.dismiss();
        }
    });
    done
}

fn footer(
    context: &Context,
    dialog: &Rc<WindowDialog>,
    details: &'static str,
    page: impl Fn() -> &'static str + 'static,
) {
    let (row, place) = button_row();
    let open = button(&context.theme, &tr(details));
    let close = context.close_sidebar.clone();
    let settings = context.settings.clone();
    open.connect_clicked(move |_| {
        close();
        settings.open(Some(page()));
    });
    row.append(&open);
    row.append(&spacer());
    row.append(&done_button(context, dialog));
    dialog.column.add(&row, place);
}

fn tinted(label: &gtk4::Label, token: &str) -> gtk4::Label {
    text::set_color(label, token);
    label.clone()
}

fn elided(label: gtk4::Label) -> gtk4::Label {
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
    let list = NetworkList::new(
        theme,
        &wifi,
        wifinetwork::Options {
            show_actions: false,
            height: None,
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

    let scanning = {
        let wifi = Rc::downgrade(&wifi);
        move || {
            if let Some(wifi) = wifi.upgrade() {
                let scanning = wifi.state.borrow().scanning;
                rule.set_visible(!scanning);
                bar.set_visible(scanning);
            }
        }
    };
    wifi.connect_changed(scanning);
    wifi.enable(true);
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

fn action_button(theme: &SharedTheme, label: &str, look: Look, colour: &str) -> RippleButton {
    let made = button(theme, label);
    made.set_look(look);
    if let Some(text) = made
        .child()
        .and_then(|holder| holder.first_child())
        .and_downcast::<gtk4::Label>()
    {
        text::set_color(&text, colour);
    }
    made
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

    let expanded: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let fill: Rc<dyn Fn()> = Rc::new({
        let theme = theme.clone();
        let bluez = bluez.clone();
        let list = list.downgrade();
        let expanded = expanded.clone();
        move || {
            let Some(list) = list.upgrade() else {
                return;
            };
            let discovering = bluez.discovering.get();
            rule.set_visible(!discovering);
            bar.set_visible(discovering);
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            for device in bluez.devices.borrow().iter() {
                list.append(&bluetooth_item(&theme, &bluez, device, &expanded));
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
    let stop = bluez.clone();
    dialog.connect_closed(move || stop.set_discovering(false));
    dialog
}

fn bluetooth_item(
    theme: &SharedTheme,
    bluez: &crate::services::bluez::Bluez,
    device: &crate::services::bluez::Device,
    expanded: &Rc<RefCell<Vec<String>>>,
) -> gtk4::Widget {
    let open = expanded.borrow().contains(&device.path);
    let item = list_item(theme, false);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    row.append(&tinted(
        &text::symbol(device_symbol(&device.icon), ICON),
        "colOnSurfaceVariant",
    ));
    let names = Column::filling_width(2);
    names.append(&elided(tinted(
        &text::styled(&if device.name.is_empty() {
            tr("Unknown device")
        } else {
            device.name.clone()
        }),
        "colOnSurfaceVariant",
    )));
    if device.paired {
        let mut status = tr(if device.connected {
            "Connected"
        } else {
            "Paired"
        });
        if let Some(battery) = device.battery {
            status.push_str(&format!(" • {}%", (battery * 100.0).round()));
        }
        names.append(&elided(tinted(
            &text::styled_sized(&status, pixel_size::SMALLER),
            "colSubtext",
        )));
    }
    let middle = Centred::filling_width(&names);
    middle.set_hexpand(true);
    row.append(&middle);
    let chevron = Centred::new(&tinted(
        &text::symbol("keyboard_arrow_down", ICON),
        "colOnLayer3",
    ));
    chevron.set_rotation(if open { 180.0 } else { 0.0 });
    row.append(&chevron);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&row);
    if open {
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        actions.set_margin_top(8);
        actions.append(&spacer());
        let paired = device.paired;
        let first = action_button(
            theme,
            &tr(if paired { "Forget" } else { "Always connect" }),
            if paired {
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
            if paired { "colOnError" } else { "colPrimary" },
        );
        let path = device.path.clone();
        first.connect_clicked({
            let bluez = bluez.clone();
            let path = path.clone();
            move |_| {
                if paired {
                    bluez.forget(&path);
                } else {
                    bluez.pair(&path);
                }
            }
        });
        actions.append(&first);
        let connected = device.connected;
        let second = action_button(
            theme,
            &tr(if connected { "Disconnect" } else { "Connect" }),
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
            move |_| bluez.connect_device(&path, !connected)
        });
        actions.append(&second);
        column.append(&actions);
    }
    item.set_content(&column, PADDING as i32, ITEM_VERTICAL);
    if open {
        item.set_cursor_from_name(None);
    }

    let flip: Rc<dyn Fn()> = {
        let expanded = expanded.clone();
        let path = device.path.clone();
        let bluez = bluez.clone();
        Rc::new(move || {
            {
                let mut open = expanded.borrow_mut();
                match open.iter().position(|entry| *entry == path) {
                    Some(index) => {
                        open.remove(index);
                    }
                    None => open.push(path.clone()),
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

fn volume(context: &Context, sink: bool) -> Rc<WindowDialog> {
    let theme = &context.theme;
    let dialog = WindowDialog::new(theme, Some(600.0));
    dialog.column.add(
        &title(&tr(if sink { "Audio output" } else { "Audio input" })),
        Place::default(),
    );
    dialog.column.add(
        &separator(),
        Place {
            top: -22.0,
            bottom: -8.0,
            fill_width: true,
            ..Place::default()
        },
    );

    let content = DialogColumn::new(16.0);
    let apps = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    apps.set_margin_top(14);
    apps.set_margin_bottom(12);
    apps.set_margin_start(20);
    apps.set_margin_end(20);
    apps.set_valign(gtk4::Align::Start);
    let empty = Placeholder::titled(
        theme,
        "widgets",
        &tr("No applications"),
        Shape::Cookie7Sided,
    );
    let stack = gtk4::Overlay::new();
    stack.set_child(Some(&scroller(&apps)));
    stack.add_overlay(&empty.widget);
    content.add(
        &stack,
        Place {
            top: -22.0,
            bottom: -16.0,
            left: -PADDING,
            right: -PADDING,
            fill_width: true,
            fill_height: true,
        },
    );
    let selector = ComboBox::new(theme);
    content.add(
        &selector.button,
        Place {
            bottom: 6.0,
            fill_width: true,
            ..Place::default()
        },
    );
    dialog.column.add(
        &content,
        Place {
            fill_width: true,
            fill_height: true,
            ..Place::default()
        },
    );
    footer(context, &dialog, "Details", || "sound");

    let Some(audio) = context.services.audio.clone() else {
        return dialog;
    };
    let devices: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    selector.connect_activated({
        let audio = audio.clone();
        let devices = devices.clone();
        move |index| {
            if let Some(name) = devices.borrow().get(index) {
                audio.set_default(sink, name);
            }
        }
    });
    let shown: Rc<RefCell<Vec<(u32, Rc<Slider>)>>> = Rc::new(RefCell::new(Vec::new()));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let audio = audio.clone();
        let theme = theme.clone();
        let apps = apps.downgrade();
        let selector = selector.clone();
        let devices = devices.clone();
        move || {
            let Some(apps) = apps.upgrade() else {
                return;
            };
            let theme = theme.clone();
            let listed = audio.clone();
            let empty = empty.clone();
            let shown = shown.clone();
            audio.streams(sink, move |streams| {
                let same = shown.borrow().len() == streams.len()
                    && shown
                        .borrow()
                        .iter()
                        .zip(&streams)
                        .all(|((index, _), stream)| *index == stream.index);
                empty.show(streams.is_empty());
                if same {
                    for ((_, slider), stream) in shown.borrow().iter().zip(&streams) {
                        slider.set(stream.volume);
                    }
                    return;
                }
                while let Some(child) = apps.first_child() {
                    apps.remove(&child);
                }
                let mut made = Vec::new();
                for stream in &streams {
                    let (row, slider) = mixer_entry(&theme, &listed, sink, stream);
                    apps.append(&row);
                    made.push((stream.index, slider));
                }
                shown.replace(made);
            });
            let selector = selector.clone();
            let devices = devices.clone();
            audio.devices(sink, move |found, current| {
                let labels: Vec<String> = found.iter().map(|device| device.label.clone()).collect();
                let index = found
                    .iter()
                    .position(|device| device.name == current)
                    .unwrap_or(0) as i32;
                devices.replace(found.into_iter().map(|device| device.name).collect());
                selector.set_items(&labels, index);
            });
        }
    });
    refresh();
    let weak = Rc::downgrade(&refresh);
    let queued = crate::ui::widgets::coalesce(Rc::new(move || {
        if let Some(refresh) = weak.upgrade() {
            refresh();
        }
    }));
    dialog.keep(Rc::new(audio.watch_nodes(move || queued())));
    dialog.keep(Rc::new(refresh));
    dialog
}

fn mixer_entry(
    theme: &SharedTheme,
    audio: &crate::services::audio::Audio,
    sink: bool,
    stream: &crate::services::audio::Stream,
) -> (gtk4::Widget, Rc<Slider>) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let icon = gtk4::Image::new();
    icon.set_pixel_size(36);
    if let Some(display) = gtk4::gdk::Display::default() {
        let icons = gtk4::IconTheme::for_display(&display);
        let preferred = crate::platform::appicon::guess(&icons, &stream.icon);
        let name = if !stream.icon.is_empty() && icons.has_icon(&preferred) {
            preferred
        } else {
            crate::platform::appicon::guess(&icons, &stream.node)
        };
        icon.set_icon_name(Some(&name));
    }
    if stream.muted {
        icon.set_opacity(0.4);
        icon.add_css_class("desaturated");
    }
    let mute = tinted(
        &text::symbol(if sink { "volume_off" } else { "mic_off" }, 22.0),
        "colOnLayer1",
    );
    mute.set_visible(stream.muted);
    let badge = gtk4::Overlay::new();
    badge.set_child(Some(&icon));
    badge.add_overlay(&Centred::new(&mute));
    badge.set_size_request(36, 36);
    badge.set_valign(gtk4::Align::Center);
    badge.set_cursor_from_name(Some("pointer"));
    let click = gtk4::GestureClick::new();
    click.connect_released({
        let audio = audio.clone();
        let (index, muted) = (stream.index, stream.muted);
        move |_, _, _, _| audio.set_stream_mute(sink, index, !muted)
    });
    badge.add_controller(click);
    let tip = Tooltip::new(&badge, theme, tooltip::Kind::Styled);
    tip.place_like_qt();
    tip.set_text(&tr(if stream.muted {
        "Click to unmute"
    } else {
        "Click to mute"
    }));
    tooltip::hover_delay(&badge, &tip, 0);
    row.append(&badge);

    let column = Column::filling_width(-4);
    column.set_hexpand(true);
    column.append(&elided(tinted(
        &text::styled(&match &stream.media {
            Some(media) => format!("{} • {media}", stream.name),
            None => stream.name.clone(),
        }),
        "colSubtext",
    )));
    let slider = small_slider(theme, 0.0, 1.0);
    slider.set_stops(vec![1.0]);
    slider.set(stream.volume);
    slider.on_moved({
        let audio = audio.clone();
        let index = stream.index;
        move |value| audio.set_stream_volume(sink, index, value)
    });
    column.append(&slider.area);
    row.append(&column);
    (row.upcast(), slider)
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
        let name = tinted(&text::styled(&tr(label)), "colSubtext");
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

fn night(context: &Context) -> Rc<WindowDialog> {
    let theme = &context.theme;
    let session = context.services.session.clone();
    let light = context.services.light.clone();
    let dialog = WindowDialog::new(theme, Some(700.0));
    let column = &dialog.column;
    let header_rule = || Place {
        top: -22.0,
        bottom: -8.0,
        fill_width: true,
        ..Place::default()
    };
    let section = Place {
        top: -16.0,
        fill_width: true,
        ..Place::default()
    };

    column.add(&title(&tr("Eye protection")), Place::default());
    column.add(&section_header(&tr("Night Light")), Place::default());
    column.add(&separator(), header_rule());

    let group = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let now = ConfigSwitch::new(theme, "check", &tr("Enable now"), {
        let session = session.clone();
        move |on| session.toggle_temperature(Some(on))
    });
    let automatic = ConfigSwitch::new(theme, "night_sight_auto", &tr("Automatic"), {
        let session = session.clone();
        move |on| session.set_automatic(on)
    });
    group.append(&now.button);
    group.append(&automatic.button);
    let (intensity, strength) = dialog_slider(theme, Some("Intensity"), 6500.0, 1200.0);
    strength.set_stops(vec![5000.0, 1200.0]);
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

    column.add(
        &section_header(&tr("Anti-flashbang (experimental)")),
        Place::default(),
    );
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
    let floor = crate::services::brightness::GAMMA_FLOOR / 100.0;
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
            fill_height: true,
            ..section
        },
    );

    let (row, place) = button_row();
    row.append(&spacer());
    row.append(&done_button(context, &dialog));
    column.add(&row, place);

    let sync: Rc<dyn Fn()> = Rc::new({
        let session = session.clone();
        let light = light.clone();
        let screen = context.screen.clone();
        move || {
            now.set(session.night.get());
            automatic.set(session.automatic.get());
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

    let refresh: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let fill: Rc<dyn Fn()> = {
        let theme = theme.clone();
        let list = list.downgrade();
        let refresh = refresh.clone();
        Rc::new(move || {
            let theme = theme.clone();
            let list = list.clone();
            let refresh = refresh.clone();
            net::tunnels(move |tunnels| {
                let Some(list) = list.upgrade() else {
                    return;
                };
                while let Some(child) = list.first_child() {
                    list.remove(&child);
                }
                for (name, up) in tunnels {
                    list.append(&wireguard_item(&theme, &name, up, &refresh));
                }
            });
        })
    };
    refresh.replace(Some(fill.clone()));
    fill();
    dialog
}

fn wireguard_item(
    theme: &SharedTheme,
    name: &str,
    up: bool,
    refresh: &Rc<RefCell<Option<Rc<dyn Fn()>>>>,
) -> gtk4::Widget {
    let item = list_item(theme, up);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let icon = customicon::build("wireguard-symbolic", ICON as i32);
    text::set_color(&icon, "colOnSurfaceVariant");
    icon.set_valign(gtk4::Align::Center);
    row.append(&icon);
    let names = Column::filling_width(2);
    names.append(&elided(tinted(&text::styled(name), "colOnSurfaceVariant")));
    names.append(&elided(tinted(
        &text::styled_sized(
            &tr(if up { "Connected" } else { "Disconnected" }),
            pixel_size::SMALLER,
        ),
        "colSubtext",
    )));
    let middle = Centred::filling_width(&names);
    middle.set_hexpand(true);
    row.append(&middle);
    if up {
        row.append(&tinted(&text::symbol("check", ICON), "colOnSurfaceVariant"));
    }
    item.set_content(&row, PADDING as i32, ITEM_VERTICAL);
    let name = name.to_owned();
    let refresh = refresh.clone();
    item.connect_clicked(move |_| {
        net::tunnel(&name, !up);
        let refresh = refresh.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
            let action = refresh.borrow().clone();
            if let Some(action) = action {
                action();
            }
        });
    });
    item.upcast()
}
