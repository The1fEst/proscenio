use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::core::{config, watch};
use crate::panels::notifications::list::CardList;
use crate::services::notifications::Notifications;
use crate::services::states::States;
use crate::ui::theme::SharedTheme;

const NAMESPACE: &str = "proscenio:notificationPopup";
const WIDTH: i32 = 410;
const SPACING: i32 = 3;
const EDGE: i32 = 4;
const ELEVATION: i32 = 10;

pub fn open(
    app: &gtk4::Application,
    notifications: &Notifications,
    events: &crate::platform::hypr::Events,
    states: &States,
    theme: &SharedTheme,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> gtk4::ApplicationWindow {
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);
    list.set_valign(gtk4::Align::Start);
    list.set_halign(gtk4::Align::End);
    list.set_margin_top(EDGE);
    list.set_margin_end(EDGE);
    list.set_margin_bottom(ELEVATION);
    list.set_size_request(WIDTH - ELEVATION * 2, -1);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .default_width(WIDTH)
        .child(&list)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_exclusive_zone(0);
    window.set_visible(false);
    window.connect_is_active_notify(|window| {
        if window.is_active() && window.keyboard_mode() == KeyboardMode::Exclusive {
            window.set_keyboard_mode(KeyboardMode::OnDemand);
        }
    });
    let leave = gtk4::EventControllerMotion::new();
    leave.connect_leave(|controller| {
        if let Some(window) = controller.widget().and_downcast::<gtk4::Window>() {
            window.set_keyboard_mode(KeyboardMode::None);
        }
    });
    window.add_controller(leave);

    let connector: String = monitor.connector().map(Into::into).unwrap_or_default();

    let cards = CardList::new(&list, notifications, theme, true);
    let rebuild: Rc<dyn Fn()> = {
        let window = window.downgrade();
        let notifications = notifications.clone();
        let states = states.clone();
        Rc::new(move || {
            let Some(window) = window
                .upgrade()
                .filter(|window| window.application().is_some())
            else {
                return;
            };
            let forced = config::value_bool("/notifications/forceMonitor/enable", false)
                .then(|| config::value_str("/notifications/forceMonitor/name"))
                .flatten()
                .filter(|name| !name.is_empty());
            let present = forced.as_ref().filter(|name| {
                gdk::Display::default().is_some_and(|display| {
                    display
                        .monitors()
                        .iter::<gdk::Monitor>()
                        .flatten()
                        .any(|monitor| monitor.connector().as_deref() == Some(name.as_str()))
                })
            });
            let mine = match present {
                Some(name) => *name == connector,
                None => {
                    crate::platform::hypr::focused_monitor().as_deref() == Some(connector.as_str())
                }
            };
            let groups = if mine {
                notifications.groups(true)
            } else {
                Vec::new()
            };

            let visible = !groups.is_empty() && !states.screen_locked.get();
            cards.update(groups);
            window.set_visible(visible);
        })
    };
    rebuild();

    let queue = crate::ui::widgets::coalesce(rebuild.clone());

    scope.keep(notifications.subscribe({
        let queue = queue.clone();
        move || queue()
    }));
    scope.keep(states.subscribe({
        let queue = queue.clone();
        move || queue()
    }));
    scope.hold(watch::config("/notifications/forceMonitor", {
        let queue = queue.clone();
        move || queue()
    }));
    scope.keep(events.subscribe(move |event, _| {
        if matches!(event, "focusedmon" | "focusedmonv2") {
            queue();
        }
    }));

    window
}
