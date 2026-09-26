use gtk4::gdk;
use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::panels::bar::traymenu;
use crate::platform::appicon;
use crate::platform::dbus;
use crate::ui::anim;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::{popup, text, tooltip};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM: &str = "org.kde.StatusNotifierItem";
const SIZE: i32 = 20;
const SPACING: i32 = 15;
const VERTICAL_SPACING: i32 = 8;
const OVERFLOW_SIZE: i32 = 24;
const OVERFLOW_SPACING: u32 = 10;

struct Tray {
    row: gtk4::Box,
    session: gio::DBusConnection,
    config: Rc<Config>,
    theme: SharedTheme,
    only: Option<&'static str>,
    vertical: bool,
    collapsed: bool,
    items: RefCell<Vec<gio::SignalSubscription>>,
}

struct Item {
    service: String,
    path: String,
    id: String,
}

pub fn build(
    session: &gio::DBusConnection,
    config: &Rc<Config>,
    theme: &SharedTheme,
    scope: &Scope,
    only: Option<&'static str>,
    vertical: bool,
    collapsed: bool,
) -> gtk4::Widget {
    let row = if vertical {
        gtk4::Box::new(gtk4::Orientation::Vertical, VERTICAL_SPACING)
    } else {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        row.set_valign(gtk4::Align::Center);
        row
    };

    register_host(session);

    let tray = Rc::new(Tray {
        row: row.clone(),
        session: session.clone(),
        config: config.clone(),
        theme: theme.clone(),
        only,
        vertical,
        collapsed,
        items: RefCell::new(Vec::new()),
    });
    refresh(&tray);
    scope.hold(watch::config("/tray/pinnedItems", {
        let tray = Rc::downgrade(&tray);
        move || {
            let Some(tray) = tray.upgrade() else {
                return;
            };
            let pinned = config::current().tray_pinned.borrow().clone();
            if *tray.config.tray_pinned.borrow() != pinned {
                tray.config.tray_pinned.replace(pinned);
                refresh(&tray);
            }
        }
    }));

    for member in [
        "StatusNotifierItemRegistered",
        "StatusNotifierItemUnregistered",
    ] {
        scope.hold(session.subscribe_to_signal(
            Some(WATCHER),
            Some(WATCHER),
            Some(member),
            Some(WATCHER_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            {
                let tray = tray.clone();
                move |_| refresh(&tray)
            },
        ));
    }
    scope.defer({
        let tray = tray.clone();
        move || {
            tray.items.take();
        }
    });

    row.upcast()
}

fn register_host(session: &gio::DBusConnection) {
    thread_local! {
        static REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if REGISTERED.replace(true) {
        return;
    }
    let name = format!("org.kde.StatusNotifierHost-{}", std::process::id());
    gio::bus_own_name_on_connection(
        session,
        &name,
        gio::BusNameOwnerFlags::NONE,
        |connection, name| {
            let name = name.to_owned();
            glib::spawn_future_local(async move {
                let _ = connection
                    .call_future(
                        Some(WATCHER),
                        WATCHER_PATH,
                        WATCHER,
                        "RegisterStatusNotifierHost",
                        Some(&(name,).to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        2000,
                    )
                    .await;
            });
        },
        |_, _| {},
    );
}

fn refresh(tray: &Rc<Tray>) {
    let tray = tray.clone();
    glib::spawn_future_local(async move {
        populate(&tray).await;
    });
}

async fn populate(tray: &Rc<Tray>) {
    tray.items.take();
    while let Some(child) = tray.row.first_child() {
        tray.row.remove(&child);
    }

    let registered = dbus::property(
        &tray.session,
        WATCHER,
        WATCHER_PATH,
        WATCHER,
        "RegisteredStatusNotifierItems",
    )
    .await;
    let Some(registered) = registered else { return };

    let mut pinned = Vec::new();
    let mut spilled = Vec::new();
    for entry in registered.iter() {
        let Some(entry) = entry.str() else { continue };
        let (service, path) = match entry.find('/') {
            Some(index) => (&entry[..index], &entry[index..]),
            None => (entry, "/StatusNotifierItem"),
        };

        if tray.config.tray_filter_passive
            && dbus::string_property(&tray.session, service, path, ITEM, "Status").await
                == Some("Passive".to_owned())
        {
            continue;
        }

        let id = dbus::string_property(&tray.session, service, path, ITEM, "Id")
            .await
            .unwrap_or_default();
        let item = Item {
            service: service.to_owned(),
            path: path.to_owned(),
            id: id.clone(),
        };
        if let Some(only) = tray.only {
            if id == only {
                pinned.push(item);
            }
        } else if tray.config.tray_shows(&id) && !tray.collapsed {
            pinned.push(item);
        } else {
            spilled.push(item);
        }
    }

    let any = !pinned.is_empty() || !spilled.is_empty();
    if !spilled.is_empty() {
        tray.row.append(&overflow(tray, spilled));
    }

    for item in &pinned {
        let child = entry(tray, item).await;
        tray.row.append(&child);
    }

    if tray.only.is_some() {
        tray.row.set_visible(any);
    } else if any {
        let separator = text::styled_sized("•", pixel_size::LARGER);
        text::set_color(&separator, "colSubtext");
        tray.row.append(&Centred::new(&separator));
    }
}

fn overflow(tray: &Rc<Tray>, items: Vec<Item>) -> gtk4::Widget {
    let chevron = text::symbol("expand_more", pixel_size::LARGER as f64);
    text::set_color(&chevron, "colOnLayer2");
    let turn = Centred::new(&chevron);

    let button = RippleButton::new(&tray.theme);
    button.set_size_request(-1, -1);
    button.set_background_size(OVERFLOW_SIZE as f32, OVERFLOW_SIZE as f32);
    button.set_look(Look {
        toggled: |theme| theme.colors.col_secondary_container,
        toggled_hover: |theme| theme.colors.col_secondary_container_hover,
        ripple_toggled: |theme| theme.colors.col_secondary_container_active,
        ..Look::default()
    });
    button.set_content(&turn, 8, 6);

    let grid = gtk4::Grid::new();
    grid.set_row_spacing(OVERFLOW_SPACING);
    grid.set_column_spacing(OVERFLOW_SPACING);

    let columns = (items.len() as f64).sqrt().ceil().max(1.0) as i32;
    glib::spawn_future_local({
        let grid = grid.clone();
        let tray = tray.clone();
        async move {
            for (index, item) in items.iter().enumerate() {
                let child = entry(&tray, item).await;
                let index = index as i32;
                grid.attach(&child, index % columns, index / columns, 1, 1);
            }
        }
    });

    let popup = popup::Popup::new(&button, popup::Bar::of(&tray.config), &grid);
    popup.popover().set_autohide(true);

    let base = match (tray.vertical, tray.config.bottom) {
        (false, _) => 0.0,
        (true, false) => -90.0,
        (true, true) => 90.0,
    };
    turn.set_rotation(base as f32);
    let rotation = anim::Motion::new(&turn, base, 200.0, anim::EXPRESSIVE_EFFECTS);
    let set_open = {
        let button = button.clone();
        let chevron = chevron.clone();
        let turn = turn.clone();
        let rotation = rotation.clone();
        let popup = popup.clone();
        move |open: bool| {
            button.set_toggled(open);
            text::set_color(
                &chevron,
                if open {
                    "colOnSecondaryContainer"
                } else {
                    "colOnLayer2"
                },
            );
            rotation.to(base + if open { 180.0 } else { 0.0 });
            let turn = turn.clone();
            let rotation = rotation.clone();
            turn.clone().add_tick_callback(move |_, _| {
                turn.set_rotation(rotation.get() as f32);
                if rotation.running() {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            });
            popup.show(open);
        }
    };
    let set_open = Rc::new(set_open);
    button.connect_down({
        let button = button.clone();
        let set_open = set_open.clone();
        move || set_open(!button.toggled())
    });
    popup.popover().connect_closed(move |_| set_open(false));

    button.upcast()
}

async fn entry(tray: &Rc<Tray>, item: &Item) -> gtk4::Widget {
    let image = gtk4::Image::new();
    image.set_pixel_size(SIZE);
    image.set_valign(gtk4::Align::Center);
    image.add_css_class("press-sink");
    let tip = tooltip::Tooltip::new(&image, &tray.theme, tooltip::Kind::Popup);
    if !tray.vertical {
        tip.below();
    }
    tip.set_text(&tooltip(tray, item).await);
    tooltip::hover_delay(&image, &tip, 0);

    let secondary = gtk4::GestureClick::new();
    secondary.set_button(gdk::BUTTON_SECONDARY);
    secondary.connect_pressed({
        let image = image.downgrade();
        let tray = tray.clone();
        let service = item.service.clone();
        let path = item.path.clone();
        let id = item.id.clone();
        move |_, _, _, _| {
            let Some(image) = image.upgrade() else {
                return;
            };
            let tray = tray.clone();
            let service = service.clone();
            let path = path.clone();
            let id = id.clone();
            glib::spawn_future_local(async move {
                let Some(menu) =
                    dbus::path_property(&tray.session, &service, &path, ITEM, "Menu").await
                else {
                    return;
                };
                let pinned = tray.config.tray_shows(&id);
                let side = match (tray.vertical, tray.config.bottom) {
                    (false, _) => gtk4::PositionType::Bottom,
                    (true, false) => gtk4::PositionType::Right,
                    (true, true) => gtk4::PositionType::Left,
                };
                traymenu::open(
                    image.upcast_ref(),
                    &tray.session,
                    &service,
                    &menu,
                    &tray.theme,
                    traymenu::Pin {
                        pinned,
                        toggle: Rc::new({
                            let tray = tray.clone();
                            move || {
                                tray.config.toggle_pin(&id);
                                refresh(&tray);
                            }
                        }),
                    },
                    side,
                );
            });
        }
    });
    image.add_controller(secondary);

    let redraw = {
        let image = image.downgrade();
        let tray = Rc::downgrade(tray);
        let service = item.service.clone();
        let path = item.path.clone();
        move || {
            let (Some(image), Some(tray)) = (image.upgrade(), tray.upgrade()) else {
                return;
            };
            let service = service.clone();
            let path = path.clone();
            glib::spawn_future_local(async move {
                show_icon(&image, &tray, &service, &path).await;
            });
        }
    };
    redraw();

    tray.items
        .borrow_mut()
        .push(tray.session.subscribe_to_signal(
            Some(&item.service),
            Some(ITEM),
            None,
            Some(&item.path),
            None,
            gio::DBusSignalFlags::NONE,
            move |_| redraw(),
        ));

    image.upcast()
}

async fn tooltip(tray: &Rc<Tray>, item: &Item) -> String {
    let detail = dbus::property(&tray.session, &item.service, &item.path, ITEM, "ToolTip").await;
    let (title, description) = match &detail {
        Some(value) if value.n_children() >= 4 => (
            value.child_value(2).str().unwrap_or_default().to_owned(),
            value.child_value(3).str().unwrap_or_default().to_owned(),
        ),
        _ => (String::new(), String::new()),
    };

    let mut text = title;
    if text.is_empty() {
        text = dbus::string_property(&tray.session, &item.service, &item.path, ITEM, "Title")
            .await
            .unwrap_or_default();
    }
    if text.is_empty() {
        text = item.id.clone();
    }
    if !description.is_empty() {
        text.push_str(" • ");
        text.push_str(&description);
    }
    if tray.config.tray_show_item_id {
        text.push_str(&format!("\n[{}]", item.id));
    }
    text
}

async fn show_icon(image: &gtk4::Image, tray: &Rc<Tray>, service: &str, path: &str) {
    let Some(paintable) = icon_for(&tray.session, service, path).await else {
        image.set_icon_name(Some("image-missing"));
        return;
    };
    if !tray.config.tray_monochrome_icons {
        image.set_paintable(Some(&paintable));
        return;
    }
    let tint = tray.theme.borrow().colors.col_on_layer0;
    match appicon::muted(image, &paintable, SIZE, tint) {
        Some(texture) => image.set_paintable(Some(&texture)),
        None => image.set_paintable(Some(&paintable)),
    }
}

async fn icon_for(
    session: &gio::DBusConnection,
    service: &str,
    path: &str,
) -> Option<gdk::Paintable> {
    let status = dbus::string_property(session, service, path, ITEM, "Status")
        .await
        .unwrap_or_else(|| "Active".to_owned());

    let attention = status == "NeedsAttention";
    let name_property = if attention {
        "AttentionIconName"
    } else {
        "IconName"
    };
    let pixmap_property = if attention {
        "AttentionIconPixmap"
    } else {
        "IconPixmap"
    };

    if let Some(theme_path) =
        dbus::string_property(session, service, path, ITEM, "IconThemePath").await
        && !theme_path.is_empty()
        && let Some(display) = gdk::Display::default()
    {
        let icons = gtk4::IconTheme::for_display(&display);
        let known = icons
            .search_path()
            .iter()
            .any(|known| known.as_os_str() == theme_path.as_str());
        if !known {
            icons.add_search_path(&theme_path);
        }
    }

    let name = dbus::string_property(session, service, path, ITEM, name_property)
        .await
        .unwrap_or_default();
    if !name.is_empty() {
        let display = gdk::Display::default()?;
        return Some(appicon::themed(
            &gtk4::IconTheme::for_display(&display),
            &name,
            "image-missing",
            SIZE,
            1,
        ));
    }

    dbus::property(session, service, path, ITEM, pixmap_property)
        .await
        .and_then(|pixmaps| texture(&pixmaps))
        .map(gdk::Texture::upcast)
}

fn texture(pixmaps: &Variant) -> Option<gdk::Texture> {
    let best = pixmaps
        .iter()
        .max_by_key(|entry| entry.child_value(0).get::<i32>().unwrap_or(0))?;
    let width = best.child_value(0).get::<i32>()?;
    let height = best.child_value(1).get::<i32>()?;
    let data: Vec<u8> = best.child_value(2).iter().filter_map(|b| b.get()).collect();
    if width <= 0 || height <= 0 || data.len() < (width * height * 4) as usize {
        return None;
    }
    Some(
        gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::A8r8g8b8,
            &glib::Bytes::from_owned(data),
            (width * 4) as usize,
        )
        .upcast(),
    )
}
