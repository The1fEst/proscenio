use gtk4::gdk::{self, RGBA};
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::i18n::tr;
use crate::core::process;
use crate::core::scope::Scope;
use crate::platform::{appicon, grab};
use crate::services::Services;
use crate::services::mpris::{Mpris, Track};
use crate::ui::anim::{EMPHASIZED_DECEL, Fade, Tween};
use crate::ui::theme::{self, Adapted, SharedTheme, mix, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::coalesce;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::progress::{self, ProgressBar};
use crate::ui::widgets::ripple::{Palette, RippleButton};
use crate::ui::widgets::slider::{self, Options, Slider};
use crate::ui::widgets::text;

const NAMESPACE: &str = "proscenio:mediaControls";
const WIDTH: i32 = 440;
const HEIGHT: i32 = 160;
const OSD_WIDTH: i32 = 180;
const ELEVATION: i32 = 10;
const GAPS_OUT: i32 = 5;
const RADIUS: f32 = (rounding::SCREEN_ROUNDING - GAPS_OUT + 1) as f32;
const ART_RADIUS: f32 = rounding::VERYSMALL as f32;
const PADDING: i32 = 13;
const ART_SIDE: i32 = HEIGHT - (ELEVATION + PADDING) * 2;
const SPACING: i32 = 15;
const INFO_SPACING: i32 = 2;
const ROW_SPACING: i32 = 5;
const TIME_GAP: i32 = 5;
const TRANSPORT: i32 = 24;
const PLAY: i32 = 44;
const SLIDER_HEIGHT: i32 = 36;
const APP_ICON_SHARE: f32 = 0.6;
const ICON: f64 = pixel_size::HUGE as f64;
const PLACEHOLDER_PADDING: i32 = 20;
const PLACEHOLDER_SPACING: i32 = 5;
const ART_MILLIS: f64 = 400.0;
const ART_BLUR: f32 = 70.0;
const WAVE_BLUR: f32 = 7.0;
const SATURATION: f32 = 0.2;
const SCRIM: f32 = 0.3;
const WAVE_ALPHA: f32 = 0.15;
const VISUALIZER_MAX: f64 = 1000.0;
const SMOOTHING: i64 = 2;
const DOMINANT_PART: f32 = 0.8;

pub struct MediaControls {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    cava: RefCell<Option<process::Running>>,
    points: Rc<RefCell<Vec<f64>>>,
    cards: Rc<RefCell<Vec<Rc<Card>>>>,
}

impl MediaControls {
    pub fn is_open(&self) -> bool {
        self.window.is_visible()
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn open(self: &Rc<Self>) {
        if self.window.is_visible() {
            return;
        }
        self.window.set_visible(true);
        self.start_cava();
        let (Some(grab), Some(surface)) = (self.grab.as_ref(), self.window.surface()) else {
            return;
        };
        let panel = self.clone();
        grab.hold(&surface, move || panel.close());
    }

    pub fn close(&self) {
        if let Some(grab) = self.grab.as_ref() {
            grab.release();
        }
        self.window.set_visible(false);
        if let Some(mut running) = self.cava.take() {
            running.stop();
        }
        self.points.borrow_mut().clear();
    }

    fn start_cava(self: &Rc<Self>) {
        let config = crate::core::paths::runtime().join("cava.conf");
        let _ = std::fs::create_dir_all(crate::core::paths::runtime());
        if std::fs::write(&config, crate::core::assets::CAVA).is_err() {
            return;
        }
        let mut command =
            process::command(&[OsStr::new("cava"), OsStr::new("-p"), config.as_os_str()]);
        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let Some(mut running) = process::start(command) else {
            return;
        };
        let Some(stdout) = running.child.stdout.take() else {
            return;
        };
        let points = self.points.clone();
        let cards = Rc::downgrade(&self.cards);
        process::lines(stdout, move |line| {
            let Some(line) = line else {
                points.borrow_mut().clear();
                return;
            };
            let Some(cards) = cards.upgrade() else {
                return;
            };
            points.replace(
                line.split(';')
                    .filter_map(|point| point.trim().parse::<f64>().ok())
                    .collect(),
            );
            for card in cards.borrow().iter() {
                card.backdrop.queue_draw();
            }
        });
        self.cava.replace(Some(running));
    }
}

pub fn build(
    app: &gtk4::Application,
    config: &Rc<Config>,
    services: &Rc<Services>,
    theme: &SharedTheme,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Rc<MediaControls> {
    let column = gtk4::Fixed::new();
    column.set_size_request(WIDTH, -1);
    column.set_valign(gtk4::Align::Start);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&column)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    if config.vertical {
        let side = if config.bottom {
            Edge::Right
        } else {
            Edge::Left
        };
        window.set_anchor(Edge::Top, true);
        window.set_anchor(side, true);
        window.set_margin(
            Edge::Top,
            (monitor.geometry().height() as f64 / 2.0 - HEIGHT as f64 * 1.5) as i32,
        );
        window.set_margin(side, config.vertical_bar_width());
    } else {
        let edge = if config.bottom {
            Edge::Bottom
        } else {
            Edge::Top
        };
        window.set_anchor(edge, true);
        window.set_anchor(Edge::Left, true);
        window.set_margin(edge, config.bar_height());
        window.set_margin(
            Edge::Left,
            monitor.geometry().width() / 2 - OSD_WIDTH / 2 - WIDTH,
        );
    }
    window.set_exclusive_zone(-1);
    window.set_visible(false);

    let controls = Rc::new(MediaControls {
        window,
        grab: grab::Grab::new(&monitor.display()),
        cava: RefCell::new(None),
        points: Rc::new(RefCell::new(Vec::new())),
        cards: Rc::new(RefCell::new(Vec::new())),
    });

    let placeholder = placeholder();
    let rebuild: Rc<dyn Fn()> = {
        let controls = Rc::downgrade(&controls);
        let mpris = services.mpris.clone();
        let theme = theme.clone();
        Rc::new(move || {
            let Some(controls) = controls.upgrade() else {
                return;
            };
            let players = mpris.meaningful();
            let mut kept: Vec<Rc<Card>> = Vec::new();
            let mut old = controls.cards.take();
            for track in &players {
                let card = match old.iter().position(|card| card.bus == track.bus) {
                    Some(index) => old.remove(index),
                    None => Card::new(&mpris, &theme, &controls.points, track),
                };
                card.update(&mpris, &theme, track);
                kept.push(card);
            }
            for card in old {
                column.remove(&card.root);
            }
            let stride = (HEIGHT - ELEVATION) as f64;
            for (index, card) in kept.iter().enumerate() {
                let y = index as f64 * stride;
                if card.root.parent().is_none() {
                    column.put(&card.root, 0.0, y);
                } else {
                    column.move_(&card.root, 0.0, y);
                }
            }
            match (kept.is_empty(), placeholder.parent().is_some()) {
                (true, false) => column.put(&placeholder, GAPS_OUT as f64, 0.0),
                (false, true) => column.remove(&placeholder),
                _ => {}
            }
            controls.cards.replace(kept);
        })
    };
    rebuild();
    scope.keep(services.mpris.subscribe({
        let queue = coalesce(rebuild);
        move || queue()
    }));

    controls
}

fn placeholder() -> gtk4::Widget {
    let title = text::styled_sized(&tr("No active player"), pixel_size::LARGE);
    title.set_xalign(0.0);
    let hint = text::styled(&tr("Make sure your player has MPRIS support"));
    text::set_color(&hint, "colSubtext");
    hint.set_xalign(0.0);

    let inside = gtk4::Box::new(gtk4::Orientation::Vertical, PLACEHOLDER_SPACING);
    inside.set_margin_top(PLACEHOLDER_PADDING);
    inside.set_margin_bottom(PLACEHOLDER_PADDING);
    inside.set_margin_start(PLACEHOLDER_PADDING);
    inside.set_margin_end(PLACEHOLDER_PADDING);
    let title_holder = Centred::new(&title);
    title_holder.set_halign(gtk4::Align::Start);
    inside.append(&title_holder);
    inside.append(&hint);

    let background = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    background.add_css_class("media-placeholder");
    background.set_margin_top(ELEVATION / 2);
    background.set_margin_bottom(ELEVATION / 2);
    background.set_margin_start(ELEVATION / 2);
    background.set_margin_end(ELEVATION / 2);
    background.append(&inside);
    background.upcast()
}

struct Look {
    adapted: Cell<Adapted>,
    texture: RefCell<Option<gdk::Texture>>,
    thumbnail: RefCell<Option<gdk::Texture>>,
    blurred: RefCell<Option<Blurred>>,
    app_icon: RefCell<Option<gdk::Paintable>>,
    opacity: Cell<Tween>,
    playing: Cell<bool>,
}

struct Blurred {
    source: gdk::Texture,
    width: f32,
    height: f32,
    scrim: RGBA,
    layer: gdk::Texture,
}

struct Card {
    bus: String,
    root: gtk4::Overlay,
    backdrop: Paint,
    art: Paint,
    look: Rc<Look>,
    track: RefCell<Track>,
    art_url: RefCell<Option<String>>,
    dominant: Cell<Option<RGBA>>,
    title: gtk4::Label,
    set_title: Rc<dyn Fn(&str)>,
    artist: gtk4::Label,
    set_artist: Rc<dyn Fn(&str)>,
    time: gtk4::Label,
    time_holder: Centred,
    row: gtk4::Box,
    has_metadata: Cell<bool>,
    play: RippleButton,
    play_icon: gtk4::Label,
    play_tint: Rc<Tint>,
    previous: RippleButton,
    next: RippleButton,
    transport_tints: [Rc<Tint>; 2],
    slider: Rc<Slider>,
    progress: ProgressBar,
    bottom_base: gtk4::Box,
    seekable: Cell<bool>,
    ticking: Cell<bool>,
}

impl Card {
    fn new(
        mpris: &Mpris,
        theme: &SharedTheme,
        points: &Rc<RefCell<Vec<f64>>>,
        track: &Track,
    ) -> Rc<Self> {
        let initial = theme
            .borrow()
            .adapted(theme.borrow().m3.secondary_container);
        let look = Rc::new(Look {
            adapted: Cell::new(initial),
            texture: RefCell::new(None),
            thumbnail: RefCell::new(None),
            blurred: RefCell::new(None),
            app_icon: RefCell::new(None),
            opacity: Cell::new(Tween::new(0.0, ART_MILLIS, EMPHASIZED_DECEL)),
            playing: Cell::new(false),
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

        let title = text::styled_sized("", pixel_size::LARGE);
        title.set_xalign(0.0);
        title.set_ellipsize(pango::EllipsizeMode::End);
        let (title_holder, set_title) = text::animate_change_sideways(&title);
        let artist = text::styled_sized("", pixel_size::SMALLER);
        artist.set_xalign(0.0);
        artist.set_ellipsize(pango::EllipsizeMode::End);
        let (artist_holder, set_artist) = text::animate_change_sideways(&artist);

        let time = text::styled("");
        let time_holder = Centred::new(&time);

        let (previous, previous_icon) = button(theme, "skip_previous", TRANSPORT);
        let (next, next_icon) = button(theme, "skip_next", TRANSPORT);
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
        let progress = ProgressBar::new();
        progress.set_valign(gtk4::Align::Center);
        progress.set_hexpand(true);
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        container.set_hexpand(true);
        container.append(&slider.area);
        container.append(&progress);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
        row.append(&previous);
        row.append(&container);
        row.append(&next);

        let (play, play_icon) = button(theme, "play_arrow", PLAY);

        let bottom_base = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let bottom = gtk4::Overlay::new();
        bottom.set_child(Some(&bottom_base));
        bottom.add_overlay(&time_holder);
        bottom.add_overlay(&row);
        bottom.add_overlay(&play);

        let info = gtk4::Box::new(gtk4::Orientation::Vertical, INFO_SPACING);
        info.set_hexpand(true);
        info.append(&title_holder);
        info.append(&artist_holder);
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
            dominant: Cell::new(None),
            title,
            set_title,
            artist,
            set_artist,
            time,
            time_holder: time_holder.clone(),
            row: row.clone(),
            has_metadata: Cell::new(true),
            play: play.clone(),
            play_icon: play_icon.clone(),
            play_tint: Tint::new(&play_icon),
            previous: previous.clone(),
            next: next.clone(),
            transport_tints: [Tint::new(&previous_icon), Tint::new(&next_icon)],
            slider: slider.clone(),
            progress,
            bottom_base,
            seekable: Cell::new(true),
            ticking: Cell::new(false),
        });

        bottom.connect_get_child_position({
            let card = Rc::downgrade(&card);
            let time = time_holder.clone().upcast::<gtk4::Widget>();
            let row = row.clone().upcast::<gtk4::Widget>();
            move |bottom, child| {
                let card = card.upgrade()?;
                let (width, height) = (bottom.width(), bottom.height());
                let row_height = card.row_height();
                let row_top = height - row_height;
                if *child == row {
                    return Some(gdk::Rectangle::new(0, row_top, width, row_height));
                }
                if *child == time {
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

        play.connect_down({
            let card = Rc::downgrade(&card);
            let mpris = mpris.clone();
            move || {
                if let Some(card) = card.upgrade() {
                    mpris.toggle_playing(&card.track.borrow());
                }
            }
        });
        previous.connect_down({
            let card = Rc::downgrade(&card);
            let mpris = mpris.clone();
            move || {
                if let Some(card) = card.upgrade() {
                    mpris.previous(&card.track.borrow());
                }
            }
        });
        next.connect_down({
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
        if self.seekable.get() {
            SLIDER_HEIGHT
        } else {
            TRANSPORT
        }
    }

    fn update(self: &Rc<Self>, mpris: &Mpris, theme: &SharedTheme, track: &Track) {
        self.track.replace(track.clone());
        let animate = self.root.is_mapped();
        self.follow_art(theme, &track.art);

        let title = clean_title(&track.title);
        let has_metadata = !title.is_empty() || !track.artist.is_empty();
        let title = if has_metadata {
            title.as_str()
        } else {
            track.identity.as_str()
        };
        let untitled = tr("Untitled");
        let title = if title.is_empty() { &untitled } else { title };
        change(&self.title, &self.set_title, title, animate);
        if self.has_metadata.replace(has_metadata) != has_metadata {
            self.time_holder.set_visible(has_metadata);
            self.row.set_visible(has_metadata);
            self.bottom_base.queue_allocate();
        }
        self.follow_app_icon(track, has_metadata);
        change(&self.artist, &self.set_artist, &track.artist, animate);

        let length = mpris.track_length(track);
        let has_length = length > 0;
        self.time.set_text(&if has_length {
            format!("{} / {}", friendly(track.position), friendly(length))
        } else {
            friendly(track.position)
        });

        let seekable = track.can_seek && has_length;
        let part = if has_length {
            track.position as f64 / length as f64
        } else {
            0.0
        };
        self.slider.area.set_visible(seekable);
        self.progress.set_visible(!seekable);
        if self.seekable.replace(seekable) != seekable {
            self.bottom_base.queue_allocate();
        }
        self.bottom_base
            .set_size_request(-1, text_height(&self.time) + self.row_height());
        if seekable {
            self.slider.set(part);
        } else {
            self.progress.set_value(part);
        }
        self.progress.set_wavy(track.playing);

        self.look.playing.set(track.playing);
        self.play_icon
            .set_text(if track.playing { "pause" } else { "play_arrow" });
        self.play.set_radius(if track.playing {
            rounding::NORMAL as f64
        } else {
            PLAY as f64 / 2.0
        });
        self.recolour(animate);
    }

    fn recolour(&self, animate: bool) {
        let colours = self.look.adapted.get();
        let playing = self.look.playing.get();
        paint_label(&self.title, colours.col_on_layer0);
        paint_label(&self.artist, colours.col_subtext);
        paint_label(&self.time, colours.col_subtext);
        self.play.set_palette(if playing {
            Palette {
                background: colours.col_primary,
                hover: colours.col_primary_hover,
                ripple: colours.col_primary_active,
            }
        } else {
            Palette {
                background: colours.col_secondary_container,
                hover: colours.col_secondary_container_hover,
                ripple: colours.col_secondary_container_active,
            }
        });
        self.play_tint.set(
            if playing {
                colours.col_on_primary
            } else {
                colours.col_on_secondary_container
            },
            animate,
        );
        let transport = Palette {
            background: transparentize(colours.col_secondary_container, 1.0),
            hover: colours.col_secondary_container_hover,
            ripple: colours.col_secondary_container_active,
        };
        self.previous.set_palette(transport);
        self.next.set_palette(transport);
        for tint in &self.transport_tints {
            tint.set(colours.col_on_secondary_container, animate);
        }
        self.slider.set_colours(slider::Colours {
            highlight: colours.col_primary,
            track: colours.col_secondary_container,
            handle: colours.col_primary,
        });
        self.progress.set_colours(progress::Colours {
            highlight: colours.col_primary,
            track: colours.col_secondary_container,
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
        let paintable = appicon::themed(
            &icons,
            &appicon::guess(&icons, app),
            "multimedia-player",
            side.round() as i32,
            self.art.scale_factor(),
        );
        self.look.app_icon.replace(Some(paintable));
        self.art.queue_draw();
    }

    fn follow_art(self: &Rc<Self>, theme: &SharedTheme, url: &str) {
        let known = self.art_url.borrow().as_deref() == Some(url);
        if !known {
            self.art_url.replace(Some(url.to_owned()));
            self.dominant.set(None);
            self.look.texture.replace(None);
            self.look.thumbnail.replace(None);
            self.look.blurred.replace(None);
            self.fade_art(false);
            if !url.is_empty() {
                self.download(theme, url);
            }
        }
        let theme_ref = theme.borrow();
        let colour = match (url.is_empty(), self.dominant.get()) {
            (true, _) => theme_ref.m3.secondary_container,
            (false, Some(average)) => mix(
                average,
                theme_ref.colors.col_primary_container,
                DOMINANT_PART,
            ),
            (false, None) => mix(
                theme_ref.colors.col_primary,
                theme_ref.colors.col_primary_container,
                DOMINANT_PART,
            ),
        };
        self.look.adapted.set(theme_ref.adapted(colour));
    }

    fn download(self: &Rc<Self>, theme: &SharedTheme, url: &str) {
        let cache = crate::core::paths::cache().join("coverart");
        let _ = std::fs::create_dir_all(&cache);
        let file = cache.join(
            glib::compute_checksum_for_string(glib::ChecksumType::Md5, url)
                .map(|sum| sum.to_string())
                .unwrap_or_default(),
        );
        let card = Rc::downgrade(self);
        let theme = theme.clone();
        let wanted = url.to_owned();
        let finish = move |file: PathBuf| {
            let card = card.clone();
            let theme = theme.clone();
            let wanted = wanted.clone();
            let scale = card.upgrade().map_or(1, |card| card.art.scale_factor());
            glib::spawn_future_local(async move {
                let sizes = vec![
                    (
                        (WIDTH - 2 * ELEVATION) * scale,
                        (HEIGHT - 2 * ELEVATION) * scale,
                    ),
                    (ART_SIDE * scale, ART_SIDE * scale),
                ];
                let loaded = crate::ui::image::cover_textures(file, sizes, |pixbuf| {
                    theme::average(pixbuf.clone())
                })
                .await;
                let Some(card) = card.upgrade() else {
                    return;
                };
                if card.art_url.borrow().as_deref() != Some(wanted.as_str()) {
                    return;
                }
                let (textures, dominant) = loaded.unzip();
                let mut textures = textures.unwrap_or_default().into_iter();
                card.dominant.set(dominant.flatten());
                card.look.texture.replace(textures.next());
                card.look.thumbnail.replace(textures.next());
                card.follow_art(&theme, &wanted);
                card.recolour(card.root.is_mapped());
                card.fade_art(true);
            });
        };
        if file.exists() {
            finish(file);
            return;
        }
        let download = process::quiet(&[
            OsStr::new("curl"),
            OsStr::new("-4"),
            OsStr::new("-sSL"),
            OsStr::new(url),
            OsStr::new("-o"),
            file.as_os_str(),
        ]);
        glib::spawn_future_local(async move {
            if process::finish(download).await.is_some() {
                finish(file);
            }
        });
    }

    fn fade_art(self: &Rc<Self>, shown: bool) {
        let target = if shown { 1.0 } else { 0.0 };
        let mut opacity = self.look.opacity.get();
        if self.root.is_mapped() {
            opacity.retarget(target, glib::monotonic_time());
        } else {
            opacity.jump(target);
        }
        self.look.opacity.set(opacity);
        self.backdrop.queue_draw();
        self.art.queue_draw();
        if self.ticking.replace(true) {
            return;
        }
        let card = Rc::downgrade(self);
        self.root.add_tick_callback(move |_, _| {
            let Some(card) = card.upgrade() else {
                return glib::ControlFlow::Break;
            };
            card.backdrop.queue_draw();
            card.art.queue_draw();
            if card.look.opacity.get().running(glib::monotonic_time()) {
                return glib::ControlFlow::Continue;
            }
            card.ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

struct Tint {
    label: gtk4::Label,
    fade: Fade,
    ticking: Cell<bool>,
}

impl Tint {
    fn new(label: &gtk4::Label) -> Rc<Self> {
        Rc::new(Tint {
            label: label.clone(),
            fade: Fade::new(),
            ticking: Cell::new(false),
        })
    }

    fn set(self: &Rc<Self>, colour: RGBA, animate: bool) {
        let now = now(&self.label);
        self.fade.retarget(colour, now, animate);
        self.apply(now);
        if !self.fade.running(now) || self.ticking.replace(true) {
            return;
        }
        let tint = Rc::downgrade(self);
        self.label.add_tick_callback(move |_, clock| {
            let Some(tint) = tint.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            tint.apply(now);
            if tint.fade.running(now) {
                return glib::ControlFlow::Continue;
            }
            tint.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn apply(&self, now: i64) {
        if let Some(colour) = self.fade.value(now) {
            paint_label(&self.label, colour);
        }
    }
}

fn paint_label(label: &gtk4::Label, colour: RGBA) {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
    let attributes = label.attributes().unwrap_or_default();
    attributes.change(pango::AttrColor::new_foreground(
        channel(colour.red()),
        channel(colour.green()),
        channel(colour.blue()),
    ));
    attributes.change(pango::AttrInt::new_foreground_alpha(channel(
        colour.alpha(),
    )));
    label.set_attributes(Some(&attributes));
}

fn change(label: &gtk4::Label, set: &Rc<dyn Fn(&str)>, value: &str, animate: bool) {
    if animate {
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
    button.set_content(&Centred::new(&symbol), 0, 0);
    (button, symbol)
}

fn paint_backdrop(
    snapshot: &gtk4::Snapshot,
    (width, height): (f32, f32),
    scale: i32,
    look: &Look,
    points: &[f64],
    renderer: Option<&gsk::Renderer>,
) {
    let bounds = graphene::Rect::new(0.0, 0.0, width, height);
    let adapted = look.adapted.get();
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, RADIUS));
    let mut base = adapted.col_layer0;
    base.set_alpha(1.0);
    snapshot.append_color(&base, &bounds);

    let opacity = look
        .opacity
        .get()
        .value(glib::monotonic_time())
        .clamp(0.0, 1.0);
    if let Some(texture) = look.texture.borrow().as_ref()
        && opacity > 0.0
    {
        snapshot.push_opacity(opacity);
        let scrim = transparentize(adapted.col_layer0, SCRIM);
        match blurred_art(look, texture, bounds, scrim, renderer) {
            Some(layer) => snapshot.append_texture(&layer, &bounds),
            None => blur_art(snapshot, texture, bounds, scrim),
        }
        snapshot.pop();
    }

    if look.playing.get()
        && points.len() >= 2
        && let Some((texture, area)) = blurred_wave(
            (width, height),
            scale,
            points,
            saturated(adapted.col_primary),
        )
    {
        snapshot.append_texture(&texture, &area);
    }
    snapshot.pop();
}

fn blurred_wave(
    (width, height): (f32, f32),
    scale: i32,
    points: &[f64],
    colour: RGBA,
) -> Option<(gdk::Texture, graphene::Rect)> {
    let scale = scale.max(1) as f64;
    let sigma = WAVE_BLUR as f64 / 2.0 * scale;
    let pad = (3.0 * sigma).ceil() as i32;
    let mask_width = (width as f64 * scale).ceil() as i32 + 2 * pad;
    let mask_height = (height as f64 * scale).ceil() as i32 + 2 * pad;
    let mut mask =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::A8, mask_width, mask_height).ok()?;
    {
        let cr = gtk4::cairo::Context::new(&mask).ok()?;
        cr.translate(pad as f64, pad as f64);
        cr.scale(scale, scale);
        wave(&cr, width as f64, height as f64, points);
        cr.fill().ok()?;
    }
    mask.flush();
    let stride = mask.stride() as usize;
    let mut coverage = mask.data().ok()?.to_vec();
    crate::ui::image::gaussian_blur(
        &mut coverage,
        (mask_width as usize, mask_height as usize),
        stride,
        1,
        sigma,
    );
    let tint = [colour.red(), colour.green(), colour.blue(), 1.0];
    let mut pixels = Vec::with_capacity((mask_width * mask_height * 4) as usize);
    for row in coverage.chunks(stride).take(mask_height as usize) {
        for &value in &row[..mask_width as usize] {
            let alpha = value as f32 / 255.0 * WAVE_ALPHA;
            pixels.extend(tint.map(|channel| (channel * alpha * 255.0).round() as u8));
        }
    }
    let texture = gdk::MemoryTexture::new(
        mask_width,
        mask_height,
        gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &glib::Bytes::from_owned(pixels),
        (mask_width * 4) as usize,
    );
    let area = graphene::Rect::new(
        (-pad as f64 / scale) as f32,
        (-pad as f64 / scale) as f32,
        (mask_width as f64 / scale) as f32,
        (mask_height as f64 / scale) as f32,
    );
    Some((texture.upcast(), area))
}

fn blur_art(
    snapshot: &gtk4::Snapshot,
    texture: &gdk::Texture,
    bounds: graphene::Rect,
    scrim: RGBA,
) {
    push_saturation(snapshot);
    snapshot.push_blur(ART_BLUR as f64);
    snapshot.push_clip(&bounds);
    snapshot.append_texture(texture, &cover(texture, bounds));
    snapshot.pop();
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, RADIUS));
    snapshot.append_color(&scrim, &bounds);
    snapshot.pop();
    snapshot.pop();
    snapshot.pop();
}

fn blurred_art(
    look: &Look,
    texture: &gdk::Texture,
    bounds: graphene::Rect,
    scrim: RGBA,
    renderer: Option<&gsk::Renderer>,
) -> Option<gdk::Texture> {
    let (width, height) = (bounds.width(), bounds.height());
    if let Some(held) = look.blurred.borrow().as_ref()
        && held.source == *texture
        && held.width == width
        && held.height == height
        && held.scrim == scrim
    {
        return Some(held.layer.clone());
    }
    let renderer = renderer?;
    let blurred = gtk4::Snapshot::new();
    blur_art(&blurred, texture, bounds, scrim);
    let layer = renderer.render_texture(blurred.to_node()?, Some(&bounds));
    look.blurred.replace(Some(Blurred {
        source: texture.clone(),
        width,
        height,
        scrim,
        layer: layer.clone(),
    }));
    Some(layer)
}

fn wave(cr: &gtk4::cairo::Context, width: f64, height: f64, points: &[f64]) {
    let count = points.len() as i64;
    cr.move_to(0.0, height);
    for index in 0..count {
        let mut sum = 0.0;
        for offset in -SMOOTHING..=SMOOTHING {
            sum += points[(index + offset).clamp(0, count - 1) as usize];
        }
        let average = sum / (SMOOTHING * 2 + 1) as f64;
        let x = index as f64 * width / (count - 1) as f64;
        cr.line_to(x, height - average / VISUALIZER_MAX * height);
    }
    cr.line_to(width, height);
    cr.close_path();
}

fn cover_art(snapshot: &gtk4::Snapshot, width: f32, height: f32, look: &Look) {
    let bounds = graphene::Rect::new(0.0, 0.0, width, height);
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, ART_RADIUS));
    snapshot.append_color(&transparentize(look.adapted.get().col_layer1, 0.5), &bounds);
    let opacity = look
        .opacity
        .get()
        .value(glib::monotonic_time())
        .clamp(0.0, 1.0);
    if let Some(texture) = look.thumbnail.borrow().as_ref()
        && opacity > 0.0
    {
        snapshot.push_opacity(opacity);
        snapshot.append_texture(texture, &cover(texture, bounds));
        snapshot.pop();
    }
    if let Some(icon) = look.app_icon.borrow().as_ref() {
        let side = height * APP_ICON_SHARE;
        snapshot.save();
        snapshot.translate(&graphene::Point::new(
            ((width - side) / 2.0).round(),
            ((height - side) / 2.0).round(),
        ));
        icon.snapshot(snapshot, side as f64, side as f64);
        snapshot.restore();
    }
    snapshot.pop();
}

fn cover(texture: &gdk::Texture, bounds: graphene::Rect) -> graphene::Rect {
    let (image_width, image_height) = (texture.width() as f32, texture.height() as f32);
    let scale = (bounds.width() / image_width).max(bounds.height() / image_height);
    let (width, height) = (image_width * scale, image_height * scale);
    graphene::Rect::new(
        (bounds.width() - width) / 2.0,
        (bounds.height() - height) / 2.0,
        width,
        height,
    )
}

fn push_saturation(snapshot: &gtk4::Snapshot) {
    snapshot.push_color_matrix(&saturation(), &graphene::Vec4::zero());
}

fn saturated(colour: RGBA) -> RGBA {
    let [red, green, blue, alpha] = saturation()
        .transform_vec4(&graphene::Vec4::new(
            colour.red(),
            colour.green(),
            colour.blue(),
            colour.alpha(),
        ))
        .to_float();
    RGBA::new(
        red.clamp(0.0, 1.0),
        green.clamp(0.0, 1.0),
        blue.clamp(0.0, 1.0),
        alpha,
    )
}

fn saturation() -> graphene::Matrix {
    let amount = 1.0 + SATURATION;
    let grey = [0.299, 0.587, 0.114].map(|weight| weight * (1.0 - amount));
    graphene::Matrix::from_float([
        grey[0] + amount,
        grey[0],
        grey[0],
        0.0,
        grey[1],
        grey[1] + amount,
        grey[1],
        0.0,
        grey[2],
        grey[2],
        grey[2] + amount,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ])
}

pub fn clean_title(title: &str) -> String {
    let mut title = title.to_owned();
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        let trimmed = title.trim_start_matches(' ');
        if let Some(rest) = trimmed.strip_prefix(open)
            && let Some(end) = rest.find(close)
        {
            title = format!(
                " {}",
                rest[end + close.len_utf8()..].trim_start_matches(' ')
            );
        }
    }
    for (open, close) in [('【', '】'), ('《', '》'), ('「', '」'), ('『', '』')] {
        let trimmed = title.trim_start_matches(' ');
        if let Some(rest) = trimmed.strip_prefix(open)
            && let Some(end) = rest.find(close)
        {
            title = rest[end + close.len_utf8()..].to_owned();
        }
    }
    title.trim().to_owned()
}

fn friendly(micros: i64) -> String {
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
        .map(|holder| holder.measure(gtk4::Orientation::Vertical, -1).1)
        .unwrap_or(0)
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturating_a_colour_keeps_greys_and_pushes_hues_away_from_grey() {
        let grey = saturated(RGBA::new(0.4, 0.4, 0.4, 0.7));
        assert!((grey.red() - 0.4).abs() < 1e-5 && (grey.blue() - 0.4).abs() < 1e-5);
        assert_eq!(grey.alpha(), 0.7);
        let purple = saturated(RGBA::new(0.6, 0.3, 0.7, 1.0));
        assert!(purple.red() > 0.6 && purple.green() < 0.3 && purple.blue() > 0.7);
    }
}
