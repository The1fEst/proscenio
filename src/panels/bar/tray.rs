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
    contents: RefCell<Contents>,
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

#[derive(Clone, Debug, PartialEq)]
struct Item {
    service: String,
    path: String,
    id: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Layout {
    overflow: Vec<Item>,
    inline: Vec<Item>,
}

#[derive(Debug, PartialEq)]
struct LayoutDiff {
    is_overflow_kept: bool,
    removed: Vec<Item>,
    added: Vec<Item>,
}

fn diff_layouts(current: &Layout, wanted: &Layout) -> LayoutDiff {
    let mut added = Vec::new();
    for item in &wanted.inline {
        if !current.inline.contains(item) && !added.contains(item) {
            added.push(item.clone());
        }
    }
    LayoutDiff {
        is_overflow_kept: current.overflow == wanted.overflow,
        removed: current
            .inline
            .iter()
            .filter(|item| !wanted.inline.contains(item))
            .cloned()
            .collect(),
        added,
    }
}

struct InlineSlot {
    item: Item,
    widget: gtk4::Widget,
    _subscription: gio::SignalSubscription,
}

struct OverflowSlot {
    items: Vec<Item>,
    widget: gtk4::Widget,
    _subscriptions: Rc<RefCell<Vec<gio::SignalSubscription>>>,
}

#[derive(Default)]
struct Contents {
    overflow: Option<OverflowSlot>,
    inline: Vec<InlineSlot>,
    separator: Option<gtk4::Widget>,
}

impl Contents {
    fn layout(&self) -> Layout {
        Layout {
            overflow: self
                .overflow
                .as_ref()
                .map(|slot| slot.items.clone())
                .unwrap_or_default(),
            inline: self.inline.iter().map(|slot| slot.item.clone()).collect(),
        }
    }
}

fn take_slot(slots: &mut Vec<InlineSlot>, item: &Item) -> Option<InlineSlot> {
    let index = slots.iter().position(|slot| slot.item == *item)?;
    Some(slots.swap_remove(index))
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
        contents: RefCell::new(Contents::default()),
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
            tray.contents.take();
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

    let mut wanted = Layout::default();
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
                wanted.inline.push(item);
            }
        } else if tray.config.tray_shows(&id) && !tray.is_collapsed {
            wanted.inline.push(item);
        } else {
            wanted.overflow.push(item);
        }
    }

    if !tray.is_current(generation) {
        return;
    }
    let diff = diff_layouts(&tray.contents.borrow().layout(), &wanted);

    let mut built = Vec::new();
    for item in &diff.added {
        let (widget, subscription) = entry(tray, item).await;
        if !tray.is_current(generation) {
            return;
        }
        built.push(InlineSlot {
            item: item.clone(),
            widget,
            _subscription: subscription,
        });
    }

    apply(tray, wanted, &diff, built);
}

fn apply(tray: &Rc<Tray>, wanted: Layout, diff: &LayoutDiff, mut built: Vec<InlineSlot>) {
    let mut contents = tray.contents.borrow_mut();
    let contents = &mut *contents;

    if !diff.is_overflow_kept {
        if let Some(slot) = contents.overflow.take() {
            tray.row.remove(&slot.widget);
        }
        if !wanted.overflow.is_empty() {
            contents.overflow = Some(overflow(tray, wanted.overflow.clone()));
        }
    }

    let (mut kept, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut contents.inline)
        .into_iter()
        .partition(|slot| !diff.removed.contains(&slot.item));
    for slot in &gone {
        tray.row.remove(&slot.widget);
    }
    contents.inline = wanted
        .inline
        .iter()
        .filter_map(|item| take_slot(&mut kept, item).or_else(|| take_slot(&mut built, item)))
        .collect();

    let has_items = !wanted.inline.is_empty() || !wanted.overflow.is_empty();
    if tray.only_id.is_some() {
        tray.row.set_visible(has_items);
    }

    let mut order: Vec<gtk4::Widget> = contents
        .overflow
        .iter()
        .map(|slot| slot.widget.clone())
        .chain(contents.inline.iter().map(|slot| slot.widget.clone()))
        .collect();
    if tray.only_id.is_none() && has_items {
        let separator = contents.separator.get_or_insert_with(|| {
            let bullet = text::styled_sized("•", pixel_size::LARGER);
            text::set_color(&bullet, "colSubtext");
            Centered::new(&bullet).upcast()
        });
        order.push(separator.clone());
    } else if let Some(separator) = contents.separator.take() {
        tray.row.remove(&separator);
    }

    let mut previous: Option<gtk4::Widget> = None;
    for widget in order {
        if widget.parent().is_none() {
            tray.row.append(&widget);
        }
        let expected = match &previous {
            Some(previous) => previous.next_sibling(),
            None => tray.row.first_child(),
        };
        if expected.as_ref() != Some(&widget) {
            tray.row.reorder_child_after(&widget, previous.as_ref());
        }
        previous = Some(widget);
    }
}

fn overflow(tray: &Rc<Tray>, items: Vec<Item>) -> OverflowSlot {
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
    let subscriptions = Rc::new(RefCell::new(Vec::new()));
    glib::spawn_future_local({
        let grid = grid.downgrade();
        let tray = Rc::downgrade(tray);
        let subscriptions = Rc::downgrade(&subscriptions);
        let items = items.clone();
        async move {
            for (index, item) in items.iter().enumerate() {
                let (Some(tray), Some(grid)) = (tray.upgrade(), grid.upgrade()) else {
                    return;
                };
                let (child, subscription) = entry(&tray, item).await;
                let Some(subscriptions) = subscriptions.upgrade() else {
                    return;
                };
                subscriptions.borrow_mut().push(subscription);
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

    OverflowSlot {
        items,
        widget: button.upcast(),
        _subscriptions: subscriptions,
    }
}

async fn entry(tray: &Rc<Tray>, item: &Item) -> (gtk4::Widget, gio::SignalSubscription) {
    let tooltip_text = tooltip(tray, item).await;
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

    let refresh_tooltip = {
        let weak_tip = Rc::downgrade(&tip);
        let weak_tray = Rc::downgrade(tray);
        let item = item.clone();
        let tooltip_generation = Rc::new(Cell::new(0u64));
        move || {
            let (Some(tip), Some(tray)) = (weak_tip.upgrade(), weak_tray.upgrade()) else {
                return;
            };
            let request_generation = tooltip_generation.get() + 1;
            tooltip_generation.set(request_generation);
            let tooltip_generation = tooltip_generation.clone();
            let item = item.clone();
            glib::spawn_future_local(async move {
                let text = tooltip(&tray, &item).await;
                if tooltip_generation.get() == request_generation {
                    tip.set_text(&text);
                }
            });
        }
    };

    let subscription = tray.session.subscribe_to_signal(
        Some(&item.service),
        Some(ITEM),
        None,
        Some(&item.path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            if matches!(signal.signal_name, "NewToolTip" | "NewTitle") {
                refresh_tooltip();
            }
            redraw();
        },
    );

    (image.upcast(), subscription)
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

    fn build_item(name: &str, id: &str) -> Item {
        Item {
            service: format!(":1.{name}"),
            path: "/StatusNotifierItem".to_owned(),
            id: id.to_owned(),
        }
    }

    #[test]
    fn an_unchanged_layout_keeps_every_item_and_the_overflow() {
        let layout = Layout {
            overflow: vec![build_item("1", "chat")],
            inline: vec![build_item("2", "sync"), build_item("3", "input")],
        };
        assert_eq!(
            diff_layouts(&layout, &layout.clone()),
            LayoutDiff {
                is_overflow_kept: true,
                removed: Vec::new(),
                added: Vec::new(),
            }
        );
    }

    #[test]
    fn an_app_adding_its_icon_adds_only_that_item() {
        let current = Layout {
            overflow: Vec::new(),
            inline: vec![build_item("1", "chat"), build_item("2", "sync")],
        };
        let wanted = Layout {
            overflow: Vec::new(),
            inline: vec![
                build_item("1", "chat"),
                build_item("2", "sync"),
                build_item("3", "input"),
            ],
        };
        assert_eq!(
            diff_layouts(&current, &wanted),
            LayoutDiff {
                is_overflow_kept: true,
                removed: Vec::new(),
                added: vec![build_item("3", "input")],
            }
        );
    }

    #[test]
    fn an_app_removing_its_icon_removes_only_that_item() {
        let current = Layout {
            overflow: Vec::new(),
            inline: vec![
                build_item("1", "chat"),
                build_item("2", "sync"),
                build_item("3", "input"),
            ],
        };
        let wanted = Layout {
            overflow: Vec::new(),
            inline: vec![build_item("1", "chat"), build_item("3", "input")],
        };
        assert_eq!(
            diff_layouts(&current, &wanted),
            LayoutDiff {
                is_overflow_kept: true,
                removed: vec![build_item("2", "sync")],
                added: Vec::new(),
            }
        );
    }

    #[test]
    fn an_item_that_changes_its_id_or_moves_to_the_overflow_is_rebuilt() {
        let current = Layout {
            overflow: Vec::new(),
            inline: vec![build_item("1", "chat"), build_item("2", "sync")],
        };
        let wanted = Layout {
            overflow: vec![build_item("1", "chat")],
            inline: vec![build_item("2", "synchronizer")],
        };
        assert_eq!(
            diff_layouts(&current, &wanted),
            LayoutDiff {
                is_overflow_kept: false,
                removed: vec![build_item("1", "chat"), build_item("2", "sync")],
                added: vec![build_item("2", "synchronizer")],
            }
        );
    }

    #[test]
    fn an_item_registered_twice_is_added_once() {
        let wanted = Layout {
            overflow: Vec::new(),
            inline: vec![build_item("1", "chat"), build_item("1", "chat")],
        };
        assert_eq!(
            diff_layouts(&Layout::default(), &wanted).added,
            vec![build_item("1", "chat")]
        );
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
