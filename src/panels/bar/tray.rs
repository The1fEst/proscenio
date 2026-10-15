use gtk4::gdk;
use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::panels::bar::traymenu;
use crate::platform::dbus;
use crate::platform::system::appicon;
use crate::ui::anim;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::{display::text, overlay::popup, overlay::tooltip};

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
    only_id: Option<&'static str>,
    is_vertical: bool,
    is_collapsed: bool,
    items: RefCell<Vec<gio::SignalSubscription>>,
    is_populating: Cell<bool>,
    needs_refresh: Cell<bool>,
    generation: Cell<u64>,
    is_closed: Cell<bool>,
}

impl Tray {
    fn is_current(&self, generation: u64) -> bool {
        !self.is_closed.get() && self.generation.get() == generation
    }
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
    only_id: Option<&'static str>,
    is_vertical: bool,
    is_collapsed: bool,
) -> gtk4::Widget {
    let row = if is_vertical {
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
        only_id,
        is_vertical,
        is_collapsed,
        items: RefCell::new(Vec::new()),
        is_populating: Cell::new(false),
        needs_refresh: Cell::new(false),
        generation: Cell::new(0),
        is_closed: Cell::new(false),
    });
    refresh(&tray);
    scope.hold(watch::config("/tray/pinnedItems", {
        let tray = Rc::downgrade(&tray);
        move || {
            let Some(tray) = tray.upgrade() else {
                return;
            };
            let pinned_ids = config::current().tray_pinned.borrow().clone();
            if *tray.config.tray_pinned.borrow() != pinned_ids {
                tray.config.tray_pinned.replace(pinned_ids);
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
            tray.is_closed.set(true);
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
    tray.generation.set(tray.generation.get() + 1);
    if tray.is_populating.replace(true) {
        tray.needs_refresh.set(true);
        return;
    }
    let tray = tray.clone();
    glib::spawn_future_local(async move {
        loop {
            tray.needs_refresh.set(false);
            populate(&tray).await;
            if !tray.needs_refresh.get() {
                break;
            }
        }
        tray.is_populating.set(false);
    });
}

async fn populate(tray: &Rc<Tray>) {
    if tray.is_closed.get() {
        return;
    }
    let generation = tray.generation.get();
    tray.items.take();
    while let Some(child) = tray.row.first_child() {
        tray.row.remove(&child);
    }

    let registered_items = dbus::property(
        &tray.session,
        WATCHER,
        WATCHER_PATH,
        WATCHER,
        "RegisteredStatusNotifierItems",
    )
    .await;
    let Some(registered_items) = registered_items else {
        return;
    };

    let mut inline_items = Vec::new();
    let mut overflow_items = Vec::new();
    for entry in registered_items.iter() {
        if !tray.is_current(generation) {
            return;
        }
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
        if let Some(only_id) = tray.only_id {
            if id == only_id {
                inline_items.push(item);
            }
        } else if tray.config.tray_shows(&id) && !tray.is_collapsed {
            inline_items.push(item);
        } else {
            overflow_items.push(item);
        }
    }

    if !tray.is_current(generation) {
        return;
    }
    let has_items = !inline_items.is_empty() || !overflow_items.is_empty();
    if !overflow_items.is_empty() {
        tray.row.append(&overflow(tray, overflow_items, generation));
    }

    for item in &inline_items {
        let Some(child) = entry(tray, item, generation).await else {
            return;
        };
        tray.row.append(&child);
    }

    if tray.only_id.is_some() {
        tray.row.set_visible(has_items);
    } else if has_items {
        let separator = text::styled_sized("•", pixel_size::LARGER);
        text::set_color(&separator, "colSubtext");
        tray.row.append(&Centered::new(&separator));
    }
}

fn overflow(tray: &Rc<Tray>, items: Vec<Item>, generation: u64) -> gtk4::Widget {
    let chevron = text::symbol("expand_more", pixel_size::LARGER as f64);
    text::set_color(&chevron, "colOnLayer2");
    let chevron_holder = Centered::new(&chevron);

    let button = RippleButton::new(&tray.theme);
    button.set_size_request(-1, -1);
    button.set_background_size(OVERFLOW_SIZE as f32, OVERFLOW_SIZE as f32);
    button.set_look(Look {
        toggled: |theme| theme.colors.col_secondary_container,
        toggled_hover: |theme| theme.colors.col_secondary_container_hover,
        ripple_toggled: |theme| theme.colors.col_secondary_container_active,
        ..Look::default()
    });
    button.set_content(&chevron_holder, 8, 6);

    let grid = gtk4::Grid::new();
    grid.set_row_spacing(OVERFLOW_SPACING);
    grid.set_column_spacing(OVERFLOW_SPACING);

    let columns = (items.len() as f64).sqrt().ceil().max(1.0) as i32;
    glib::spawn_future_local({
        let grid = grid.downgrade();
        let tray = Rc::downgrade(tray);
        async move {
            for (index, item) in items.iter().enumerate() {
                let (Some(tray), Some(grid)) = (tray.upgrade(), grid.upgrade()) else {
                    return;
                };
                let Some(child) = entry(&tray, item, generation).await else {
                    return;
                };
                let index = index as i32;
                grid.attach(&child, index % columns, index / columns, 1, 1);
            }
        }
    });

    let popup = popup::Popup::new(&button, popup::Bar::of(&tray.config), &grid);
    popup.popover().set_autohide(true);

    let base = match (tray.is_vertical, tray.config.bottom) {
        (false, _) => 0.0,
        (true, false) => -90.0,
        (true, true) => 90.0,
    };
    chevron_holder.set_rotation(base as f32);
    let rotation = anim::Motion::new(&chevron_holder, base, 200.0, anim::EXPRESSIVE_EFFECTS);
    let set_open = {
        let button = button.downgrade();
        let chevron = chevron.clone();
        let chevron_holder = chevron_holder.clone();
        let rotation = rotation.clone();
        move |is_open: bool| {
            let Some(button) = button.upgrade() else {
                return;
            };
            button.set_toggled(is_open);
            text::set_color(
                &chevron,
                if is_open {
                    "colOnSecondaryContainer"
                } else {
                    "colOnLayer2"
                },
            );
            rotation.to(base + if is_open { 180.0 } else { 0.0 });
            let rotation = rotation.clone();
            chevron_holder.add_tick_callback(move |chevron_holder, _| {
                chevron_holder.set_rotation(rotation.get() as f32);
                if rotation.is_running() {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            });
        }
    };
    let set_open = Rc::new(set_open);
    button.connect_down({
        let button = button.downgrade();
        let set_open = set_open.clone();
        let popup = popup.clone();
        move || {
            let Some(button) = button.upgrade() else {
                return;
            };
            let is_open = !button.is_toggled();
            set_open(is_open);
            popup.show(is_open);
        }
    });
    popup.popover().connect_closed(move |_| set_open(false));

    button.upcast()
}

async fn entry(tray: &Rc<Tray>, item: &Item, generation: u64) -> Option<gtk4::Widget> {
    let tooltip_text = tooltip(tray, item).await;
    if !tray.is_current(generation) {
        return None;
    }
    let image = gtk4::Image::new();
    image.set_pixel_size(SIZE);
    image.set_valign(gtk4::Align::Center);
    image.add_css_class("press-sink");
    let tip = tooltip::Tooltip::new(&image, &tray.theme, tooltip::Kind::Popup);
    if !tray.is_vertical {
        tip.below();
    }
    tip.set_text(&tooltip_text);
    tooltip::hover_delay(&image, &tip, 0);

    let secondary_gesture = gtk4::GestureClick::new();
    secondary_gesture.set_button(gdk::BUTTON_SECONDARY);
    secondary_gesture.connect_pressed({
        let image = image.downgrade();
        let tray = Rc::downgrade(tray);
        let service = item.service.clone();
        let path = item.path.clone();
        let id = item.id.clone();
        move |_, _, _, _| {
            let (Some(image), Some(tray)) = (image.upgrade(), tray.upgrade()) else {
                return;
            };
            let service = service.clone();
            let path = path.clone();
            let id = id.clone();
            glib::spawn_future_local(async move {
                let Some(menu) =
                    dbus::path_property(&tray.session, &service, &path, ITEM, "Menu").await
                else {
                    return;
                };
                let is_pinned = tray.config.tray_shows(&id);
                let side = match (tray.is_vertical, tray.config.bottom) {
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
                        is_pinned,
                        on_toggle: Rc::new({
                            let config = tray.config.clone();
                            let tray = Rc::downgrade(&tray);
                            move || {
                                config.toggle_pin(&id);
                                if let Some(tray) = tray.upgrade() {
                                    refresh(&tray);
                                }
                            }
                        }),
                    },
                    side,
                );
            });
        }
    });
    image.add_controller(secondary_gesture);

    let redraw = {
        let weak_image = image.downgrade();
        let weak_tray = Rc::downgrade(tray);
        let service = item.service.clone();
        let path = item.path.clone();
        let icon_generation = Rc::new(Cell::new(0u64));
        move || {
            let (Some(image), Some(tray)) = (weak_image.upgrade(), weak_tray.upgrade()) else {
                return;
            };
            if !image.is_realized() {
                return;
            }
            let scale = image.scale_factor();
            let session = tray.session.clone();
            let request_generation = icon_generation.get() + 1;
            icon_generation.set(request_generation);
            let icon_generation = icon_generation.clone();
            let weak_image = weak_image.clone();
            let weak_tray = weak_tray.clone();
            let service = service.clone();
            let path = path.clone();
            glib::spawn_future_local(async move {
                let paintable = icon_for(&session, &service, &path, scale).await;
                if let (Some(image), Some(tray)) = (weak_image.upgrade(), weak_tray.upgrade())
                    && image.is_realized()
                    && icon_generation.get() == request_generation
                {
                    show_icon(&image, &tray, paintable);
                }
            });
        }
    };
    image.connect_realize({
        let redraw = redraw.clone();
        move |_| redraw()
    });
    image.connect_scale_factor_notify({
        let redraw = redraw.clone();
        move |_| redraw()
    });

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

    Some(image.upcast())
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

fn show_icon(image: &gtk4::Image, tray: &Tray, paintable: Option<gdk::Paintable>) {
    let Some(paintable) = paintable else {
        image.set_icon_name(Some("image-missing"));
        return;
    };
    if !tray.config.tray_monochrome_icons {
        image.set_paintable(Some(&paintable));
        return;
    }
    let tint = tray.theme.borrow().colors.col_on_layer0;
    match appicon::render_muted(image, &paintable, SIZE, tint) {
        Some(texture) => image.set_paintable(Some(&texture)),
        None => image.set_paintable(Some(&paintable)),
    }
}

async fn icon_for(
    session: &gio::DBusConnection,
    service: &str,
    path: &str,
    scale: i32,
) -> Option<gdk::Paintable> {
    let status = dbus::string_property(session, service, path, ITEM, "Status")
        .await
        .unwrap_or_else(|| "Active".to_owned());

    let needs_attention = status == "NeedsAttention";
    let name_property = if needs_attention {
        "AttentionIconName"
    } else {
        "IconName"
    };
    let pixmap_property = if needs_attention {
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
        let is_known = icons
            .search_path()
            .iter()
            .any(|search_path| search_path.as_os_str() == theme_path.as_str());
        if !is_known {
            icons.add_search_path(&theme_path);
        }
    }

    let name = dbus::string_property(session, service, path, ITEM, name_property)
        .await
        .unwrap_or_default();
    if !name.is_empty() {
        let display = gdk::Display::default()?;
        return Some(appicon::load_icon(
            &gtk4::IconTheme::for_display(&display),
            &name,
            "image-missing",
            SIZE,
            scale,
        ));
    }

    dbus::property(session, service, path, ITEM, pixmap_property)
        .await
        .and_then(|pixmaps| texture(&pixmaps, SIZE * scale))
        .map(gdk::Texture::upcast)
}

fn texture(pixmaps: &Variant, size: i32) -> Option<gdk::Texture> {
    let (width, height, data) = best_pixmap(pixmaps, size)?;
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

fn best_pixmap(pixmaps: &Variant, size: i32) -> Option<(i32, i32, Vec<u8>)> {
    if !pixmaps.is::<Vec<(i32, i32, Vec<u8>)>>() {
        return None;
    }
    let (width, height, data) = pixmaps
        .iter()
        .filter_map(|entry| {
            let width = entry.child_value(0).get::<i32>()?;
            let height = entry.child_value(1).get::<i32>()?;
            let data = entry.child_value(2);
            let length = width.checked_mul(height)?.checked_mul(4)?;
            let fits =
                width > 0 && height > 0 && data.fixed_array::<u8>().ok()?.len() >= length as usize;
            fits.then_some((width, height, data))
        })
        .min_by_key(|(width, height, _)| {
            let edge = *width.max(height);
            if edge >= size {
                (false, edge)
            } else {
                (true, -edge)
            }
        })?;
    Some((width, height, data.fixed_array::<u8>().ok()?.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_square_pixmap(edge: i32, fill: u8) -> (i32, i32, Vec<u8>) {
        (edge, edge, vec![fill; (edge * edge * 4) as usize])
    }

    #[test]
    fn a_pixmap_reply_of_the_wrong_type_is_ignored() {
        assert_eq!(best_pixmap(&"pixmap".to_variant(), SIZE), None);
        assert_eq!(best_pixmap(&vec![(16i32, 16i32)].to_variant(), SIZE), None);
    }

    #[test]
    fn a_pixmap_whose_size_overflows_is_skipped() {
        let pixmaps = vec![(65536i32, 65536i32, vec![0u8; 4])].to_variant();
        assert_eq!(best_pixmap(&pixmaps, SIZE), None);
    }

    #[test]
    fn a_valid_pixmap_wins_over_a_larger_broken_one() {
        let pixmaps = vec![(64i32, 64i32, vec![0u8; 4]), build_square_pixmap(16, 7)].to_variant();
        assert_eq!(
            best_pixmap(&pixmaps, SIZE),
            Some(build_square_pixmap(16, 7))
        );
    }

    #[test]
    fn the_smallest_pixmap_that_fills_the_slot_wins() {
        let pixmaps = vec![
            build_square_pixmap(64, 1),
            build_square_pixmap(16, 2),
            build_square_pixmap(32, 3),
            build_square_pixmap(48, 4),
        ]
        .to_variant();
        assert_eq!(best_pixmap(&pixmaps, 20), Some(build_square_pixmap(32, 3)));
        assert_eq!(best_pixmap(&pixmaps, 40), Some(build_square_pixmap(48, 4)));
        assert_eq!(best_pixmap(&pixmaps, 16), Some(build_square_pixmap(16, 2)));
    }

    #[test]
    fn the_largest_pixmap_wins_when_none_fills_the_slot() {
        let pixmaps = vec![
            build_square_pixmap(16, 1),
            build_square_pixmap(22, 2),
            build_square_pixmap(8, 3),
        ]
        .to_variant();
        assert_eq!(best_pixmap(&pixmaps, 40), Some(build_square_pixmap(22, 2)));
    }
}
