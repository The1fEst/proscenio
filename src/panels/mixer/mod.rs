use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::i18n::tr;
use crate::core::scope::Scope;
use crate::panels::settings::Settings;
use crate::panels::settings::content::inputs::new_slider;
use crate::panels::sound::devices::DevicePicker;
use crate::panels::sound::streams::{
    COLUMN_SPACING, ICON_SIZE, LINE_SPACING, PERCENT, ROW_MARGIN, StreamList,
};
use crate::platform::wayland::grab;
use crate::services::Services;
use crate::services::media::audio::Audio;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::coalesce;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::{ComboBox, icon_button};
use crate::ui::widgets::input::flickable;
use crate::ui::widgets::input::slider::Slider;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::column::Column;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

const NAMESPACE: &str = "proscenio:volumeMixer";
const WIDTH: i32 = 380;
const ELEVATION: i32 = 10;
const WINDOW_WIDTH: i32 = WIDTH + ELEVATION * 2;
const SPACING: i32 = 12;
const LIST_MAX_HEIGHT: i32 = 320;
const SYMBOL_SIZE: f64 = 28.0;
const SETTINGS_PAGE: &str = "volumelevels";

struct MasterRow {
    slider: Rc<Slider>,
    mute_label: gtk4::Label,
    tip: Rc<Tooltip>,
}

pub struct VolumeMixer {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    audio: Audio,
    panel: gtk4::Box,
    chip: glib::WeakRef<gtk4::Widget>,
    monitor_size: (i32, i32),
    is_vertical: bool,
    fitted_height: Cell<i32>,
    master: MasterRow,
    combo: Rc<ComboBox>,
    picker: Rc<DevicePicker>,
    streams: Rc<StreamList>,
}

impl VolumeMixer {
    pub fn set_chip(&self, chip: &impl IsA<gtk4::Widget>) {
        self.chip.set(Some(chip.as_ref()));
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
        self.place();
        self.fit_content();
        self.refresh();
        self.window.set_visible(true);
        let (Some(grab), Some(surface)) = (self.grab.as_ref(), self.window.surface()) else {
            return;
        };
        let mixer = Rc::downgrade(self);
        grab.hold(&surface, move || {
            if let Some(mixer) = mixer.upgrade() {
                mixer.close();
            }
        });
    }

    pub fn close(&self) {
        if let Some(grab) = self.grab.as_ref() {
            grab.release();
        }
        self.window.set_visible(false);
        self.combo.close();
    }

    fn refresh(&self) {
        let percent = (self.audio.sink_volume.get() * 100.0).round();
        self.master.slider.set(percent);
        self.master.slider.set_tooltip(&format!("{percent}%"));
        let is_muted = self.audio.is_sink_muted.get();
        self.master
            .mute_label
            .set_text(if is_muted { "volume_off" } else { "volume_up" });
        self.master.tip.set_text(&tr(if is_muted {
            "Click to unmute"
        } else {
            "Click to mute"
        }));
        self.picker.refresh();
        self.streams.refresh();
    }

    fn measure_height(&self) -> i32 {
        self.panel
            .measure(gtk4::Orientation::Vertical, WINDOW_WIDTH)
            .1
    }

    fn fit_content(&self) -> bool {
        let height = self.measure_height();
        if self.fitted_height.replace(height) == height {
            return false;
        }
        self.window.set_default_size(WINDOW_WIDTH, height);
        true
    }

    fn follow_content(&self) {
        if self.fit_content() && self.is_vertical {
            self.place();
        }
    }

    fn place(&self) {
        let (width, height) = self.monitor_size;
        let bounds = self
            .chip
            .upgrade()
            .and_then(|chip| chip.compute_bounds(&chip.root()?));
        if self.is_vertical {
            let extent = self.measure_height();
            let start = bounds.map_or(f64::from(height - extent) / 2.0, |bounds| {
                f64::from(bounds.y()) - f64::from(ELEVATION)
            });
            self.window
                .set_margin(Edge::Top, clamp_start(start, extent, height));
        } else {
            let center = bounds.map_or(f64::from(width) / 2.0, |bounds| {
                f64::from(bounds.x() + bounds.width() / 2.0)
            });
            let start = center - f64::from(WINDOW_WIDTH) / 2.0;
            self.window
                .set_margin(Edge::Left, clamp_start(start, WINDOW_WIDTH, width));
        }
    }
}

fn clamp_start(start: f64, extent: i32, limit: i32) -> i32 {
    (start.round() as i32).clamp(0, (limit - extent).max(0))
}

pub fn build(
    app: &gtk4::Application,
    config: &Rc<Config>,
    services: &Rc<Services>,
    theme: &SharedTheme,
    settings: &Rc<Settings>,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Option<Rc<VolumeMixer>> {
    let audio = services.audio.clone()?;

    let combo = ComboBox::new(theme);
    combo.set_icon("speaker");
    combo.button.set_hexpand(true);
    let device_list = combo.use_inline_list();
    let picker = DevicePicker::new(combo.clone(), &audio, true);
    let (master, master_box) = build_master_row(theme, &audio);
    let streams = StreamList::new(theme, &audio, &services.mpris, true);

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(LIST_MAX_HEIGHT);
    scroll.set_child(Some(&streams.root));
    flickable::follow_scroll_settings(&scroll);

    let (levels_button, _) = icon_button(theme, "tune", false, &tr("Volume Levels"));

    let panel = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);
    panel.add_css_class("mixer-panel");
    panel.set_size_request(WIDTH, -1);
    panel.set_halign(gtk4::Align::Start);
    panel.set_valign(gtk4::Align::Start);
    panel.set_margin_top(ELEVATION);
    panel.set_margin_bottom(ELEVATION);
    panel.set_margin_start(ELEVATION);
    panel.set_margin_end(ELEVATION);
    panel.append(&combo.button);
    panel.append(&device_list);
    panel.append(&master_box);
    panel.append(&scroll);
    panel.append(&levels_button);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&panel)
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
    }
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_visible(false);

    let geometry = monitor.geometry();
    let mixer = Rc::new(VolumeMixer {
        window: window.clone(),
        grab: grab::Grab::new(&monitor.display()),
        audio: audio.clone(),
        panel,
        chip: glib::WeakRef::new(),
        monitor_size: (geometry.width(), geometry.height()),
        is_vertical: config.vertical,
        fitted_height: Cell::new(0),
        master,
        combo,
        picker,
        streams,
    });

    let follow_content: Rc<dyn Fn()> = Rc::new({
        let mixer = Rc::downgrade(&mixer);
        move || {
            if let Some(mixer) = mixer.upgrade()
                && mixer.window.is_visible()
            {
                mixer.follow_content();
            }
        }
    });
    mixer.streams.connect_updated({
        let follow_content = follow_content.clone();
        move || follow_content()
    });
    mixer.picker.connect_updated({
        let follow_content = follow_content.clone();
        move || follow_content()
    });
    device_list.connect_visible_notify(move |_| follow_content());

    mixer.master.slider.on_moved({
        let audio = audio.clone();
        let slider = Rc::downgrade(&mixer.master.slider);
        move |value| {
            if let Some(slider) = slider.upgrade() {
                slider.set_tooltip(&format!("{}%", value.round()));
            }
            audio.set_sink_volume(value.round() / 100.0);
        }
    });

    levels_button.connect_clicked({
        let mixer = Rc::downgrade(&mixer);
        let settings = settings.clone();
        move |_| {
            if let Some(mixer) = mixer.upgrade() {
                mixer.close();
            }
            settings.open(Some(SETTINGS_PAGE));
        }
    });

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    key_controller.connect_key_pressed({
        let mixer = Rc::downgrade(&mixer);
        move |_, key, _, _| {
            if key != gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if let Some(mixer) = mixer.upgrade() {
                mixer.close();
            }
            glib::Propagation::Stop
        }
    });
    window.add_controller(key_controller);

    let schedule_refresh = coalesce(Rc::new({
        let mixer = Rc::downgrade(&mixer);
        move || {
            if let Some(mixer) = mixer.upgrade()
                && mixer.window.is_visible()
            {
                mixer.refresh();
            }
        }
    }));
    scope.keep(audio.watch({
        let schedule_refresh = schedule_refresh.clone();
        move || schedule_refresh()
    }));
    scope.keep(services.mpris.subscribe(move || schedule_refresh()));

    Some(mixer)
}

fn build_master_row(theme: &SharedTheme, audio: &Audio) -> (MasterRow, gtk4::Box) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, COLUMN_SPACING);
    row.set_margin_start(ROW_MARGIN);
    row.set_margin_end(ROW_MARGIN);

    let mute_label = text::symbol("volume_up", SYMBOL_SIZE);
    text::set_color(&mute_label, "colOnSecondaryContainer");
    let badge = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    badge.append(&Centered::new(&mute_label));
    badge.set_size_request(ICON_SIZE, ICON_SIZE);
    badge.set_valign(gtk4::Align::Center);
    badge.set_cursor_from_name(Some("pointer"));
    let click_gesture = gtk4::GestureClick::new();
    click_gesture.connect_released({
        let audio = audio.clone();
        move |_, _, _, _| audio.toggle_sink_mute()
    });
    badge.add_controller(click_gesture);
    let tip = tooltip::attach_hover_tip(&badge, theme, "");
    row.append(&badge);

    let lines = Column::filling_width(LINE_SPACING);
    lines.set_hexpand(true);
    let title_label = text::styled(&tr("Volume"));
    text::set_color(&title_label, "colOnSecondaryContainer");
    title_label.set_xalign(0.0);
    lines.append(&title_label);
    let slider = new_slider(theme, PERCENT);
    lines.append(&slider.area);
    row.append(&lines);

    (
        MasterRow {
            slider,
            mute_label,
            tip,
        },
        row,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_starts_where_asked_without_leaving_the_screen() {
        for (start, extent, limit, expected) in [
            (300.0, 400, 1920, 300),
            (-20.0, 400, 1920, 0),
            (1700.0, 400, 1920, 1520),
            (500.0, 400, 300, 0),
            (299.6, 401, 1920, 300),
        ] {
            assert_eq!(clamp_start(start, extent, limit), expected);
        }
    }
}
