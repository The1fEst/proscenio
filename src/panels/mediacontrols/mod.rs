mod art;
mod card;
mod renderer;

use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::RefCell;
use std::ffi::OsStr;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::i18n::tr;
use crate::core::process;
use crate::core::scope::Scope;
use crate::platform::wayland::grab;
use crate::services::Services;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::coalesce;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use card::Card;

const NAMESPACE: &str = "proscenio:mediaControls";
const WIDTH: i32 = 440;
const HEIGHT: i32 = 160;
const OSD_WIDTH: i32 = 180;
const ELEVATION: i32 = 10;
const GAPS_OUT: i32 = 5;
const PADDING: i32 = 13;
const ART_SIDE: i32 = HEIGHT - (ELEVATION + PADDING) * 2;
const APP_ICON_SHARE: f32 = 0.6;
const PLACEHOLDER_PADDING: i32 = 20;
const PLACEHOLDER_SPACING: i32 = 5;

pub struct MediaControls {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    cava_process: RefCell<Option<process::Running>>,
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
        let panel = Rc::downgrade(self);
        grab.hold(&surface, move || {
            if let Some(panel) = panel.upgrade() {
                panel.close();
            }
        });
    }

    pub fn close(&self) {
        if let Some(grab) = self.grab.as_ref() {
            grab.release();
        }
        self.window.set_visible(false);
        self.stop_cava();
        self.points.borrow_mut().clear();
    }

    fn stop_cava(&self) {
        if let Some(mut cava_process) = self.cava_process.take() {
            cava_process.stop();
        }
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
        let Some(mut cava_process) = process::start(command) else {
            return;
        };
        let Some(stdout) = cava_process.child.stdout.take() else {
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
        self.cava_process.replace(Some(cava_process));
    }
}

impl Drop for MediaControls {
    fn drop(&mut self) {
        self.stop_cava();
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
        cava_process: RefCell::new(None),
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
            let players = mpris.distinct_tracks();
            let mut current_cards: Vec<Rc<Card>> = Vec::new();
            let mut old = controls.cards.take();
            for track in &players {
                let card = match old.iter().position(|card| card.bus == track.bus) {
                    Some(index) => old.remove(index),
                    None => Card::new(&mpris, &theme, &controls.points, track),
                };
                card.update(&mpris, &theme, track);
                current_cards.push(card);
            }
            for card in old {
                column.remove(&card.root);
            }
            let stride = (HEIGHT - ELEVATION) as f64;
            for (index, card) in current_cards.iter().enumerate() {
                let y = index as f64 * stride;
                if card.root.parent().is_none() {
                    column.put(&card.root, 0.0, y);
                } else {
                    column.move_(&card.root, 0.0, y);
                }
            }
            match (current_cards.is_empty(), placeholder.parent().is_some()) {
                (true, false) => column.put(&placeholder, GAPS_OUT as f64, 0.0),
                (false, true) => column.remove(&placeholder),
                _ => {}
            }
            controls.cards.replace(current_cards);
        })
    };
    rebuild();
    scope.keep(services.mpris.subscribe({
        let schedule_rebuild = coalesce(rebuild);
        move || schedule_rebuild()
    }));

    controls
}

fn placeholder() -> gtk4::Widget {
    let title_label = text::styled_sized(&tr("No active player"), pixel_size::LARGE);
    title_label.set_xalign(0.0);
    let hint_label = text::styled(&tr("Make sure your player has MPRIS support"));
    text::set_color(&hint_label, "colSubtext");
    hint_label.set_xalign(0.0);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, PLACEHOLDER_SPACING);
    content.set_margin_top(PLACEHOLDER_PADDING);
    content.set_margin_bottom(PLACEHOLDER_PADDING);
    content.set_margin_start(PLACEHOLDER_PADDING);
    content.set_margin_end(PLACEHOLDER_PADDING);
    let title_box = Centered::new(&title_label);
    title_box.set_halign(gtk4::Align::Start);
    content.append(&title_box);
    content.append(&hint_label);

    let background = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    background.add_css_class("media-placeholder");
    background.set_margin_top(ELEVATION / 2);
    background.set_margin_bottom(ELEVATION / 2);
    background.set_margin_start(ELEVATION / 2);
    background.set_margin_end(ELEVATION / 2);
    background.append(&content);
    background.upcast()
}

pub fn clean_title(title: &str) -> String {
    let mut title = title.to_owned();
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        let title_text = title.trim_start_matches(' ');
        if let Some(rest) = title_text.strip_prefix(open)
            && let Some(end) = rest.find(close)
        {
            title = format!(
                " {}",
                rest[end + close.len_utf8()..].trim_start_matches(' ')
            );
        }
    }
    for (open, close) in [('【', '】'), ('《', '》'), ('「', '」'), ('『', '』')] {
        let title_text = title.trim_start_matches(' ');
        if let Some(rest) = title_text.strip_prefix(open)
            && let Some(end) = rest.find(close)
        {
            title = rest[end + close.len_utf8()..].to_owned();
        }
    }
    title.trim().to_owned()
}
