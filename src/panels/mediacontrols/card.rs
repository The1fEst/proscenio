use gtk4::gdk::{self, RGBA};
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::renderer::{Look, cover_art, paint_backdrop};
use super::{APP_ICON_SHARE, ART_SIDE, ELEVATION, HEIGHT, PADDING, WIDTH, clean_title};
use crate::core::i18n::tr;
use crate::platform::system::appicon;
use crate::services::media::mpris::{Mpris, Track};
use crate::ui::anim::{EMPHASIZED_DECEL, Fade, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::progress::{self, ProgressBar};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Palette, RippleButton};
use crate::ui::widgets::input::slider::{self, Options, Slider};
use crate::ui::widgets::layout::centered::Centered;

const SPACING: i32 = 15;
const INFO_SPACING: i32 = 2;
const ROW_SPACING: i32 = 5;
const TIME_GAP: i32 = 5;
const TRANSPORT: i32 = 24;
const PLAY: i32 = 44;
const SLIDER_HEIGHT: i32 = 36;
const ICON: f64 = pixel_size::HUGE as f64;
const ART_MILLIS: f64 = 400.0;

pub(super) struct Card {
    pub(super) bus: String,
    pub(super) root: gtk4::Overlay,
    pub(super) backdrop: Paint,
    pub(super) art: Paint,
    pub(super) look: Rc<Look>,
    track: RefCell<Track>,
    pub(super) art_url: RefCell<Option<String>>,
    pub(super) art_retry_time_micros: Cell<Option<i64>>,
    pub(super) dominant_color: Cell<Option<RGBA>>,
    title_label: gtk4::Label,
    set_title: Rc<dyn Fn(&str)>,
    artist_label: gtk4::Label,
    set_artist: Rc<dyn Fn(&str)>,
    time_label: gtk4::Label,
    time_box: Centered,
    row: gtk4::Box,
    has_metadata: Cell<bool>,
    play_button: RippleButton,
    play_icon: gtk4::Label,
    play_tint: Rc<Tint>,
    previous_button: RippleButton,
    next_button: RippleButton,
    transport_tints: [Rc<Tint>; 2],
    slider: Rc<Slider>,
    progress_bar: ProgressBar,
    bottom_base: gtk4::Box,
    is_seekable: Cell<bool>,
    pub(super) is_ticking: Cell<bool>,
}

impl Card {
    pub(super) fn new(
        mpris: &Mpris,
        theme: &SharedTheme,
        points: &Rc<RefCell<Vec<f64>>>,
        track: &Track,
    ) -> Rc<Self> {
        let initial = theme
            .borrow()
            .adapt_colors_to(theme.borrow().m3.secondary_container);
        let look = Rc::new(Look {
            adapted_colors: Cell::new(initial),
            texture: RefCell::new(None),
            thumbnail: RefCell::new(None),
            blur_cache: RefCell::new(None),
            app_icon: RefCell::new(None),
            opacity: Cell::new(Tween::new(0.0, ART_MILLIS, EMPHASIZED_DECEL)),
            is_playing: Cell::new(false),
        });

        let backdrop = Paint::new(|_, _, _| {});
        backdrop.set_draw({
            let look = look.clone();
            let points = points.clone();
            let canvas = backdrop.downgrade();
            move |snapshot, width, height| {
                let canvas = canvas.upgrade();
                let scale = canvas.as_ref().map_or(1, |canvas| canvas.scale_factor());
                let renderer = canvas
                    .and_then(|canvas| canvas.native())
                    .and_then(|native| native.renderer());
                paint_backdrop(
                    snapshot,
                    (width, height),
                    scale,
                    &look,
                    &points.borrow(),
                    renderer.as_ref(),
                )
            }
        });
        backdrop.add_css_class("media-card");
        backdrop.set_margin_top(ELEVATION);
        backdrop.set_margin_bottom(ELEVATION);
        backdrop.set_margin_start(ELEVATION);
        backdrop.set_margin_end(ELEVATION);

        let art = Paint::new({
            let look = look.clone();
            move |snapshot, width, height| cover_art(snapshot, width, height, &look)
        });
        art.set_size_request(ART_SIDE, ART_SIDE);

        let title_label = text::styled_sized("", pixel_size::LARGE);
        title_label.set_xalign(0.0);
        title_label.set_ellipsize(pango::EllipsizeMode::End);
        let (title_box, set_title) = text::animate_change_sideways(&title_label);
        let artist_label = text::styled_sized("", pixel_size::SMALLER);
        artist_label.set_xalign(0.0);
        artist_label.set_ellipsize(pango::EllipsizeMode::End);
        let (artist_box, set_artist) = text::animate_change_sideways(&artist_label);

        let time_label = text::styled("");
        let time_box = Centered::new(&time_label);

        let (previous_button, previous_icon) = button(theme, "skip_previous", TRANSPORT);
        let (next_button, next_icon) = button(theme, "skip_next", TRANSPORT);
        let slider = Slider::with(
            theme,
            Options {
                track: slider::WAVY,
                from: 0.0,
                to: 1.0,
                icon: None,
                secondary: None,
                dividers: Vec::new(),
            },
        );
        slider.set_stops(vec![1.0]);
        let progress_bar = ProgressBar::new();
        progress_bar.set_valign(gtk4::Align::Center);
        progress_bar.set_hexpand(true);
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        container.set_hexpand(true);
        container.append(&slider.area);
        container.append(&progress_bar);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
        row.append(&previous_button);
        row.append(&container);
        row.append(&next_button);

        let (play_button, play_icon) = button(theme, "play_arrow", PLAY);

        let bottom_base = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let bottom = gtk4::Overlay::new();
        bottom.set_child(Some(&bottom_base));
        bottom.add_overlay(&time_box);
        bottom.add_overlay(&row);
        bottom.add_overlay(&play_button);

        let info = gtk4::Box::new(gtk4::Orientation::Vertical, INFO_SPACING);
        info.set_hexpand(true);
        info.append(&title_box);
        info.append(&artist_box);
        let filler = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        filler.set_vexpand(true);
        info.append(&filler);
        info.append(&bottom);

        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        let inset = ELEVATION + PADDING;
        content.set_margin_top(inset);
        content.set_margin_bottom(inset);
        content.set_margin_start(inset);
        content.set_margin_end(inset);
        content.append(&art);
        content.append(&info);

        let root = gtk4::Overlay::new();
        root.set_size_request(WIDTH, HEIGHT);
        root.set_child(Some(&backdrop));
        root.add_overlay(&content);

        let card = Rc::new(Card {
            bus: track.bus.clone(),
            root,
            backdrop,
            art,
            look,
            track: RefCell::new(track.clone()),
            art_url: RefCell::new(None),
            art_retry_time_micros: Cell::new(None),
            dominant_color: Cell::new(None),
            title_label,
            set_title,
            artist_label,
            set_artist,
            time_label,
            time_box: time_box.clone(),
            row: row.clone(),
            has_metadata: Cell::new(true),
            play_button: play_button.clone(),
            play_icon: play_icon.clone(),
            play_tint: Tint::new(&play_icon),
            previous_button: previous_button.clone(),
            next_button: next_button.clone(),
            transport_tints: [Tint::new(&previous_icon), Tint::new(&next_icon)],
            slider: slider.clone(),
            progress_bar,
            bottom_base,
            is_seekable: Cell::new(true),
            is_ticking: Cell::new(false),
        });

        bottom.connect_get_child_position({
            let card = Rc::downgrade(&card);
            let time_box = time_box.clone().upcast::<gtk4::Widget>();
            let row = row.clone().upcast::<gtk4::Widget>();
            move |bottom, child| {
                let card = card.upgrade()?;
                let (width, height) = (bottom.width(), bottom.height());
                let row_height = card.row_height();
                let row_top = height - row_height;
                if *child == row {
                    return Some(gdk::Rectangle::new(0, row_top, width, row_height));
                }
                if *child == time_box {
                    let time_width = child.measure(gtk4::Orientation::Horizontal, -1).1;
                    let time_height = child.measure(gtk4::Orientation::Vertical, -1).1;
                    return Some(gdk::Rectangle::new(
                        0,
                        row_top - TIME_GAP - time_height,
                        time_width,
                        time_height,
                    ));
                }
                let play_top = if card.has_metadata.get() {
                    row_top - TIME_GAP - PLAY
                } else {
                    height - PLAY
                };
                Some(gdk::Rectangle::new(width - PLAY, play_top, PLAY, PLAY))
            }
        });

        play_button.connect_down({
            let card = Rc::downgrade(&card);
            let mpris = mpris.clone();
            move || {
                if let Some(card) = card.upgrade() {
                    mpris.toggle_playing(&card.track.borrow());
                }
            }
        });
        previous_button.connect_down({
            let card = Rc::downgrade(&card);
            let mpris = mpris.clone();
            move || {
                if let Some(card) = card.upgrade() {
                    mpris.previous(&card.track.borrow());
                }
            }
        });
        next_button.connect_down({
            let card = Rc::downgrade(&card);
            let mpris = mpris.clone();
            move || {
                if let Some(card) = card.upgrade() {
                    mpris.next(&card.track.borrow());
                }
            }
        });
        slider.on_released({
            let card = Rc::downgrade(&card);
            let mpris = mpris.clone();
            move |value| {
                let Some(card) = card.upgrade() else {
                    return;
                };
                let track = card.track.borrow();
                let length = mpris.track_length(&track);
                mpris.seek(&track, (value * length as f64) as i64);
            }
        });

        card
    }

    fn row_height(&self) -> i32 {
        if self.is_seekable.get() {
            SLIDER_HEIGHT
        } else {
            TRANSPORT
        }
    }

    pub(super) fn update(self: &Rc<Self>, mpris: &Mpris, theme: &SharedTheme, track: &Track) {
        self.track.replace(track.clone());
        let should_animate = self.root.is_mapped();
        self.follow_art(theme, &track.art);

        let title = clean_title(&track.title);
        let has_metadata = !title.is_empty() || !track.artist.is_empty();
        let title = if has_metadata {
            title.as_str()
        } else {
            track.identity.as_str()
        };
        let untitled_text = tr("Untitled");
        let title = if title.is_empty() {
            &untitled_text
        } else {
            title
        };
        change(&self.title_label, &self.set_title, title, should_animate);
        if self.has_metadata.replace(has_metadata) != has_metadata {
            self.time_box.set_visible(has_metadata);
            self.row.set_visible(has_metadata);
            self.bottom_base.queue_allocate();
        }
        self.follow_app_icon(track, has_metadata);
        change(
            &self.artist_label,
            &self.set_artist,
            &track.artist,
            should_animate,
        );

        let length = mpris.track_length(track);
        let has_length = length > 0;
        self.time_label.set_text(&if has_length {
            format!(
                "{} / {}",
                format_duration(track.position),
                format_duration(length)
            )
        } else {
            format_duration(track.position)
        });

        let is_seekable = track.can_seek && has_length;
        let part = if has_length {
            track.position as f64 / length as f64
        } else {
            0.0
        };
        self.slider.area.set_visible(is_seekable);
        self.progress_bar.set_visible(!is_seekable);
        if self.is_seekable.replace(is_seekable) != is_seekable {
            self.bottom_base.queue_allocate();
        }
        self.bottom_base
            .set_size_request(-1, text_height(&self.time_label) + self.row_height());
        if is_seekable {
            self.slider.set(part);
        } else {
            self.progress_bar.set_value(part);
        }
        self.progress_bar.set_wavy(track.is_playing);

        self.look.is_playing.set(track.is_playing);
        self.play_icon.set_text(if track.is_playing {
            "pause"
        } else {
            "play_arrow"
        });
        self.play_button.set_radius(if track.is_playing {
            rounding::NORMAL as f64
        } else {
            PLAY as f64 / 2.0
        });
        self.recolor(should_animate);
    }

    pub(super) fn recolor(&self, should_animate: bool) {
        let colors = self.look.adapted_colors.get();
        let is_playing = self.look.is_playing.get();
        paint_label(&self.title_label, colors.col_on_layer0);
        paint_label(&self.artist_label, colors.col_subtext);
        paint_label(&self.time_label, colors.col_subtext);
        self.play_button.set_palette(if is_playing {
            Palette {
                background: colors.col_primary,
                hover: colors.col_primary_hover,
                ripple: colors.col_primary_active,
            }
        } else {
            Palette {
                background: colors.col_secondary_container,
                hover: colors.col_secondary_container_hover,
                ripple: colors.col_secondary_container_active,
            }
        });
        self.play_tint.set(
            if is_playing {
                colors.col_on_primary
            } else {
                colors.col_on_secondary_container
            },
            should_animate,
        );
        let transport_palette = Palette {
            background: transparentize(colors.col_secondary_container, 1.0),
            hover: colors.col_secondary_container_hover,
            ripple: colors.col_secondary_container_active,
        };
        self.previous_button.set_palette(transport_palette);
        self.next_button.set_palette(transport_palette);
        for tint in &self.transport_tints {
            tint.set(colors.col_on_secondary_container, should_animate);
        }
        self.slider.set_colors(slider::Colors {
            highlight: colors.col_primary,
            track: colors.col_secondary_container,
            handle: colors.col_primary,
        });
        self.progress_bar.set_colors(progress::Colors {
            highlight: colors.col_primary,
            track: colors.col_secondary_container,
        });
        self.backdrop.queue_draw();
        self.art.queue_draw();
    }

    fn follow_app_icon(&self, track: &Track, has_metadata: bool) {
        if has_metadata {
            if self.look.app_icon.replace(None).is_some() {
                self.art.queue_draw();
            }
            return;
        }
        if self.look.app_icon.borrow().is_some() {
            return;
        }
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let icons = gtk4::IconTheme::for_display(&display);
        let app = if track.desktop_entry.is_empty() {
            &track.identity
        } else {
            &track.desktop_entry
        };
        let side = ART_SIDE as f32 * APP_ICON_SHARE;
        let paintable = appicon::load_icon(
            &icons,
            &appicon::guess(&icons, app),
            "multimedia-player",
            side.round() as i32,
            self.art.scale_factor(),
        );
        self.look.app_icon.replace(Some(paintable));
        self.art.queue_draw();
    }
}

struct Tint {
    label: gtk4::Label,
    fade: Fade,
    is_ticking: Cell<bool>,
}

impl Tint {
    fn new(label: &gtk4::Label) -> Rc<Self> {
        Rc::new(Tint {
            label: label.clone(),
            fade: Fade::new(),
            is_ticking: Cell::new(false),
        })
    }

    fn set(self: &Rc<Self>, color: RGBA, should_animate: bool) {
        let now = now(&self.label);
        self.fade.retarget(color, now, should_animate);
        self.apply(now);
        if !self.fade.is_running(now) || self.is_ticking.replace(true) {
            return;
        }
        let tint = Rc::downgrade(self);
        self.label.add_tick_callback(move |_, clock| {
            let Some(tint) = tint.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            tint.apply(now);
            if tint.fade.is_running(now) {
                return glib::ControlFlow::Continue;
            }
            tint.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn apply(&self, now: i64) {
        if let Some(color) = self.fade.value(now) {
            paint_label(&self.label, color);
        }
    }
}

fn paint_label(label: &gtk4::Label, color: RGBA) {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
    let attributes = label.attributes().unwrap_or_default();
    attributes.change(pango::AttrColor::new_foreground(
        channel(color.red()),
        channel(color.green()),
        channel(color.blue()),
    ));
    attributes.change(pango::AttrInt::new_foreground_alpha(channel(color.alpha())));
    label.set_attributes(Some(&attributes));
}

fn change(label: &gtk4::Label, set: &Rc<dyn Fn(&str)>, value: &str, should_animate: bool) {
    if should_animate {
        set(value);
    } else {
        label.set_text(value);
    }
}

fn button(theme: &SharedTheme, icon: &str, size: i32) -> (RippleButton, gtk4::Label) {
    let symbol = text::symbol_filled(icon, ICON, 1.0);
    let button = RippleButton::new(theme);
    button.set_size_request(size, size);
    button.set_valign(gtk4::Align::Center);
    button.set_content(&Centered::new(&symbol), 0, 0);
    (button, symbol)
}

fn format_duration(micros: i64) -> String {
    if micros < 0 {
        return "0:00".to_owned();
    }
    let seconds = micros / 1_000_000;
    let (hours, minutes, rest) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if hours > 0 {
        return format!("{hours}:{minutes:02}:{rest:02}");
    }
    format!("{minutes}:{rest:02}")
}

fn text_height(label: &gtk4::Label) -> i32 {
    label
        .parent()
        .map(|parent| parent.measure(gtk4::Orientation::Vertical, -1).1)
        .unwrap_or(0)
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}
