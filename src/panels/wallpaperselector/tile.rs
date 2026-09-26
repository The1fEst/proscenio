use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::services::thumbnails::{self, Size};
use crate::services::wallpapers::Entry;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, Fade, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::text;

pub const MARGIN: f64 = 8.0;
pub const PADDING: f64 = 6.0;
const SPACING: i32 = 4;
const NAME_MARGIN: i32 = 10;
const APPEAR_MILLIS: f64 = 200.0;

#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Plain,
    Wallpaper,
    Current,
}

pub struct Tile {
    pub widget: gtk4::Overlay,
    pub entry: Entry,
    theme: SharedTheme,
    background: Paint,
    picture: Option<Paint>,
    name: gtk4::Label,
    fade: Fade,
    texture: RefCell<Option<gdk::Texture>>,
    appear: Cell<Tween>,
    size: Size,
}

impl Tile {
    pub fn new(
        theme: &SharedTheme,
        entry: Entry,
        (width, height): (i32, i32),
        size: Size,
    ) -> Rc<Self> {
        let background = Paint::new(|_, _, _| {});
        background.set_size_request(width, height);

        let name = text::styled_sized(&entry.name, pixel_size::SMALLER);
        name.add_css_class("wallpaper-tile-name");
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        name.set_justify(gtk4::Justification::Center);
        name.set_margin_start(NAME_MARGIN);
        name.set_margin_end(NAME_MARGIN);

        let inset = (MARGIN + PADDING) as i32;
        let name_height = name.measure(gtk4::Orientation::Vertical, -1).1;
        let area_width = width - inset * 2;
        let area_height = height - inset * 2 - name_height - SPACING;

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);
        column.set_margin_start(inset);
        column.set_margin_end(inset);
        column.set_margin_top(inset);
        column.set_margin_bottom(inset);
        column.set_can_target(false);

        let picture = entry.thumbnailed().then(|| {
            let paint = Paint::new(|_, _, _| {});
            paint.set_size_request(area_width, area_height);
            paint.add_css_class("wallpaper-thumbnail");
            column.append(&paint);
            paint
        });
        if picture.is_none() {
            let icon = gtk4::Image::from_gicon(&icon_for(&entry));
            icon.set_pixel_size(area_width.min(area_height));
            icon.set_size_request(area_width, area_height);
            column.append(&icon);
        }
        column.append(&name);

        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&background));
        widget.add_overlay(&column);
        widget.set_cursor_from_name(Some("pointer"));

        let tile = Rc::new(Tile {
            widget,
            entry,
            theme: theme.clone(),
            background: background.clone(),
            picture,
            name,
            fade: Fade::new(),
            texture: RefCell::new(None),
            appear: Cell::new(Tween::new(0.0, APPEAR_MILLIS, EXPRESSIVE_EFFECTS)),
            size,
        });

        background.set_draw({
            let tile = Rc::downgrade(&tile);
            move |snapshot, width, height| {
                let Some(tile) = tile.upgrade() else {
                    return;
                };
                let now = tile
                    .background
                    .frame_clock()
                    .map_or(0, |clock| clock.frame_time());
                let Some(colour) = tile.fade.value(now) else {
                    return;
                };
                let margin = MARGIN as f32;
                let bounds = graphene::Rect::new(
                    margin,
                    margin,
                    width - margin * 2.0,
                    height - margin * 2.0,
                );
                let radius = rounding::NORMAL as f32;
                let corner = graphene::Size::new(radius, radius);
                snapshot.push_rounded_clip(&gsk::RoundedRect::new(
                    bounds, corner, corner, corner, corner,
                ));
                snapshot.append_color(&colour, &bounds);
                snapshot.pop();
            }
        });
        if let Some(picture) = &tile.picture {
            picture.set_draw({
                let tile = Rc::downgrade(&tile);
                move |snapshot, width, height| {
                    let Some(tile) = tile.upgrade() else {
                        return;
                    };
                    tile.draw_picture(snapshot, width, height);
                }
            });
        }
        tile.set_state(State::Plain, false);
        tile
    }

    pub fn set_state(self: &Rc<Self>, state: State, animate: bool) {
        let theme = self.theme.borrow();
        let (colour, text_token) = match state {
            State::Current => (theme.colors.col_primary, "colOnPrimary"),
            State::Wallpaper => (
                theme.colors.col_secondary_container,
                "colOnSecondaryContainer",
            ),
            State::Plain => (
                transparentize(theme.colors.col_primary_container, 1.0),
                "colOnLayer0",
            ),
        };
        drop(theme);
        text::set_color(&self.name, text_token);
        let now = self
            .background
            .frame_clock()
            .map_or(0, |clock| clock.frame_time());
        self.fade
            .retarget(colour, now, animate && self.background.is_mapped());
        self.animate();
    }

    pub fn load_thumbnail(self: &Rc<Self>) {
        if self.picture.is_none() || self.texture.borrow().is_some() {
            return;
        }
        let Some(path) = thumbnails::path(&self.entry.path, self.size) else {
            return;
        };
        if !path.is_file() {
            return;
        }
        let limit = self.size.pixels();
        let tile = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Some(texture) =
                crate::ui::image::texture(PathBuf::from(&path), (limit, limit)).await
            else {
                return;
            };
            let Some(tile) = tile.upgrade() else {
                return;
            };
            tile.texture.replace(Some(texture));
            if let Some(picture) = &tile.picture {
                picture.add_css_class("loaded");
                let now = picture.frame_clock().map_or(0, |clock| clock.frame_time());
                let mut appear = tile.appear.get();
                if picture.is_mapped() {
                    appear.retarget(1.0, now);
                } else {
                    appear.jump(1.0);
                }
                tile.appear.set(appear);
            }
            tile.animate();
        });
    }

    fn draw_picture(&self, snapshot: &gtk4::Snapshot, width: f32, height: f32) {
        let texture = self.texture.borrow();
        let Some(texture) = texture.as_ref() else {
            return;
        };
        let now = self
            .picture
            .as_ref()
            .and_then(|picture| picture.frame_clock())
            .map_or(0, |clock| clock.frame_time());
        let opacity = self.appear.get().value(now);
        let (texture_width, texture_height) = (texture.width() as f32, texture.height() as f32);
        let scale = (width / texture_width).max(height / texture_height);
        let (drawn_width, drawn_height) = (texture_width * scale, texture_height * scale);
        let area = graphene::Rect::new(0.0, 0.0, width, height);
        let radius = rounding::SMALL as f32;
        let corner = graphene::Size::new(radius, radius);
        snapshot.push_rounded_clip(&gsk::RoundedRect::new(area, corner, corner, corner, corner));
        snapshot.push_opacity(opacity);
        snapshot.append_scaled_texture(
            texture,
            gsk::ScalingFilter::Linear,
            &graphene::Rect::new(
                (width - drawn_width) / 2.0,
                (height - drawn_height) / 2.0,
                drawn_width,
                drawn_height,
            ),
        );
        snapshot.pop();
        snapshot.pop();
    }

    fn animate(self: &Rc<Self>) {
        self.background.queue_draw();
        if let Some(picture) = &self.picture {
            picture.queue_draw();
        }
        let tile = Rc::downgrade(self);
        self.background.add_tick_callback(move |_, clock| {
            let Some(tile) = tile.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            tile.background.queue_draw();
            if let Some(picture) = &tile.picture {
                picture.queue_draw();
            }
            if tile.fade.running(now) || tile.appear.get().running(now) {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
}

fn icon_for(entry: &Entry) -> gio::ThemedIcon {
    if entry.is_dir {
        let special = [
            glib::UserDirectory::Documents,
            glib::UserDirectory::Downloads,
            glib::UserDirectory::Music,
            glib::UserDirectory::Pictures,
            glib::UserDirectory::Videos,
        ]
        .into_iter()
        .any(|folder| glib::user_special_dir(folder).as_deref() == Some(entry.path.as_path()));
        if special {
            return gio::ThemedIcon::from_names(&[
                &format!("folder-{}", entry.name.to_lowercase()),
                "inode-directory",
            ]);
        }
        return gio::ThemedIcon::new("inode-directory");
    }
    let (mime, _) = gio::content_type_guess(Some(&entry.path), None);
    let name = gio::content_type_get_mime_type(&mime)
        .map(|mime| mime.replace('/', "-"))
        .unwrap_or_default();
    gio::ThemedIcon::from_names(&[&name, "image-missing"])
}
