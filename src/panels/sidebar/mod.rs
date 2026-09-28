pub mod classic;
pub mod dialogs;
pub mod quickpanel;
pub mod quicktoggle;
pub mod systemrow;
pub mod toggles;

use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::config::Config;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::scope::Scope;
use crate::panels::notifications::list;
use crate::platform::grab;
use crate::services::Services;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::slider::Slider;
use crate::ui::widgets::windowdialog::WindowDialog;
use quickpanel::{QuickPanel, Setup};

const NAMESPACE: &str = "proscenio:sidebarRight";
const WIDTH: i32 = 460;
const GAP: i32 = 5;
const ELEVATION: i32 = 10;
const PADDING: i32 = 10;

pub struct Sidebar {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    overlay: gtk4::Overlay,
    dialog: Rc<RefCell<Option<Rc<WindowDialog>>>>,
    watchers: Listeners<bool>,
}

impl Sidebar {
    pub fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn open(self: &Rc<Self>) {
        if !self.window.is_visible() {
            self.show();
        }
    }

    pub fn close(&self) {
        if self.window.is_visible() {
            self.hide();
        }
    }

    pub fn watch(&self, watcher: impl Fn(bool) + 'static) -> Subscription {
        watcher(self.window.is_visible());
        self.watchers.add_with(move |open| watcher(*open))
    }

    fn show(self: &Rc<Self>) {
        self.window.set_visible(true);
        self.announce();
        let (Some(grab), Some(surface)) = (self.grab.as_ref(), self.window.surface()) else {
            return;
        };
        let sidebar = self.clone();
        grab.hold(&surface, move || sidebar.hide());
    }

    fn hide(&self) {
        if let Some(grab) = self.grab.as_ref() {
            grab.release();
        }
        if let Some(dialog) = self.dialog.borrow_mut().take() {
            self.overlay.remove_overlay(&dialog.root);
        }
        self.window.set_visible(false);
        self.announce();
    }

    fn announce(&self) {
        self.watchers.notify_with(&self.window.is_visible());
    }
}

pub fn build(
    app: &gtk4::Application,
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Rc<Services>,
    session: &Rc<crate::panels::sessionscreen::SessionScreen>,
    settings: &Rc<crate::panels::settings::Settings>,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Rc<Sidebar> {
    let screen: String = monitor.connector().map(Into::into).unwrap_or_default();
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, PADDING);
    column.add_css_class("sidebar");
    column.set_margin_top(GAP);
    column.set_margin_bottom(GAP);
    column.set_margin_end(GAP);
    column.set_margin_start(ELEVATION);

    let owner: Rc<RefCell<Weak<Sidebar>>> = Rc::new(RefCell::new(Weak::new()));
    let close: Rc<dyn Fn()> = {
        let owner = owner.clone();
        Rc::new(move || {
            if let Some(sidebar) = owner.borrow().upgrade() {
                sidebar.close();
            }
        })
    };

    let panel: Rc<RefCell<Option<Rc<QuickPanel>>>> = Rc::new(RefCell::new(None));
    let editing: Rc<dyn Fn(bool)> = {
        let panel = panel.clone();
        Rc::new(move |editing| {
            if let Some(panel) = panel.borrow().as_ref() {
                panel.set_editing(editing);
            }
        })
    };
    column.append(&systemrow::build(
        theme,
        &services.background,
        systemrow::Actions {
            close: close.clone(),
            edit: (config.toggle_style == "android").then_some(editing),
            settings: settings.clone(),
        },
        session,
        scope,
    ));

    if config.quick_sliders
        && (config.slider_brightness || config.slider_volume || config.slider_mic)
    {
        column.append(&sliders(config, theme, services, &screen, scope));
    }

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&column));

    let open_dialog: Rc<RefCell<Option<Rc<WindowDialog>>>> = Rc::new(RefCell::new(None));
    if config.toggle_style == "android" || config.toggle_style == "classic" {
        let open_menu: Rc<dyn Fn(toggles::Menu)> = {
            let overlay = overlay.downgrade();
            let open = open_dialog.clone();
            let context = Rc::new(dialogs::Context {
                services: services.clone(),
                theme: theme.clone(),
                close_sidebar: close.clone(),
                settings: settings.clone(),
                screen: screen.clone(),
            });
            Rc::new(move |menu| {
                let Some(overlay) = overlay.upgrade() else {
                    return;
                };
                if let Some(previous) = open.borrow_mut().take() {
                    overlay.remove_overlay(&previous.root);
                }
                let dialog = dialogs::open(menu, &context);
                dialog.root.set_margin_top(GAP);
                dialog.root.set_margin_bottom(GAP);
                dialog.root.set_margin_end(GAP);
                dialog.root.set_margin_start(ELEVATION);
                let weak = Rc::downgrade(&dialog);
                let overlay_for_dismiss = overlay.downgrade();
                let slot = open.clone();
                dialog.connect_dismiss(move || {
                    let (Some(dialog), Some(overlay)) =
                        (weak.upgrade(), overlay_for_dismiss.upgrade())
                    else {
                        return;
                    };
                    let slot = slot.clone();
                    let root = dialog.root.clone();
                    dialog.show(false, move || {
                        overlay.remove_overlay(&root);
                        slot.borrow_mut().take();
                    });
                });
                overlay.add_overlay(&dialog.root);
                dialog.show(true, || {});
                open.replace(Some(dialog));
            })
        };
        if config.toggle_style == "classic" {
            let context = Rc::new(classic::Context {
                services: services.clone(),
                settings: settings.clone(),
                close: close.clone(),
                open_menu,
            });
            column.append(&classic::build(theme, context, scope));
        } else {
            let made = QuickPanel::new(
                services,
                theme,
                config,
                Setup {
                    width: (WIDTH - GAP - ELEVATION - PADDING * 2) as f64,
                    columns: config.toggle_columns,
                    close: close.clone(),
                    open_menu,
                },
                scope,
            );
            column.append(&made.widget());
            panel.replace(Some(made));
        }
    }

    column.append(&list::build(&services.notifications, theme, scope));

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .default_width(WIDTH)
        .child(&overlay)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_visible(false);
    crate::ui::unload::when_hidden(&window);

    let dismiss = gtk4::EventControllerKey::new();
    dismiss.connect_key_pressed({
        let close = close.clone();
        move |_, key, _, _| {
            if key != gdk::Key::Escape {
                return gtk4::glib::Propagation::Proceed;
            }
            close();
            gtk4::glib::Propagation::Stop
        }
    });
    window.add_controller(dismiss);

    let sidebar = Rc::new(Sidebar {
        window,
        grab: grab::Grab::new(&monitor.display()),
        overlay,
        dialog: open_dialog,
        watchers: Listeners::default(),
    });
    owner.replace(Rc::downgrade(&sidebar));
    sidebar
}

fn sliders(
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Rc<Services>,
    screen: &str,
    scope: &Scope,
) -> gtk4::Widget {
    let group = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    group.add_css_class("sidebar-group");

    if config.slider_brightness {
        let light = services.light.clone();
        let screen = screen.to_owned();
        let slider = Slider::new(theme, "light_mode", Some(("wb_twilight", 0.3)), vec![0.3]);
        let show: Rc<dyn Fn(&Slider)> = {
            let light = light.clone();
            let screen = screen.clone();
            Rc::new(move |slider| {
                let (level, gamma) = (light.level(&screen), light.gamma.get());
                if gamma == 100.0 {
                    slider.set(0.3 + level * 0.7);
                    slider.set_stops(Vec::new());
                    slider.set_tooltip(&format!("{}%", (level * 100.0).round()));
                    return;
                }
                slider.set(
                    (gamma - crate::services::brightness::GAMMA_FLOOR)
                        / (100.0 - crate::services::brightness::GAMMA_FLOOR)
                        * 0.3,
                );
                slider.set_stops(if level == 0.0 {
                    Vec::new()
                } else {
                    vec![0.3 + level * 0.7]
                });
                slider.set_tooltip(&format!("Gamma {gamma}%"));
            })
        };
        show(&slider);
        scope.keep(light.subscribe({
            let slider = slider.clone();
            let show = show.clone();
            move |_| show(&slider)
        }));
        let weak = Rc::downgrade(&slider);
        slider.on_moved(move |value| {
            if value >= 0.3 {
                light.set_level(&screen, (value - 0.3) / 0.7);
                if light.gamma.get() != 100.0 {
                    light.set_gamma(100.0);
                }
            } else {
                if light.level(&screen) != 0.0 {
                    light.set_level(&screen, 0.0);
                }
                light.set_gamma(
                    value / 0.3 * (100.0 - crate::services::brightness::GAMMA_FLOOR)
                        + crate::services::brightness::GAMMA_FLOOR,
                );
            }
            if let Some(slider) = weak.upgrade() {
                show(&slider);
            }
        });
        group.append(&slider.area);
    }

    if let Some(audio) = &services.audio {
        if config.slider_volume {
            let slider = Slider::new(theme, "volume_up", None, Vec::new());
            slider.set(audio.sink_volume.get());
            scope.keep(audio.subscribe({
                let slider = slider.clone();
                let level = audio.sink_volume.clone();
                move || slider.set(level.get())
            }));
            slider.on_moved({
                let audio = audio.clone();
                move |value| audio.set_sink_volume(value)
            });
            group.append(&slider.area);
        }
        if config.slider_mic {
            let slider = Slider::new(theme, "mic", None, Vec::new());
            slider.set(audio.source_volume.get());
            scope.keep(audio.subscribe({
                let slider = slider.clone();
                let level = audio.source_volume.clone();
                move || slider.set(level.get())
            }));
            slider.on_moved({
                let audio = audio.clone();
                move |value| audio.set_source_volume(value)
            });
            group.append(&slider.area);
        }
    }

    group.upcast()
}
