use gtk4::glib;
use std::cell::RefCell;
use std::os::fd::AsRawFd;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, globals};

use crate::core::actions;
use crate::platform::wayland::readable;

pub mod protocol {
    use wayland_client;

    pub mod __interfaces {
        wayland_scanner::generate_interfaces!("protocols/hyprland-global-shortcuts-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-global-shortcuts-v1.xml");
}

use protocol::hyprland_global_shortcut_v1::{self, HyprlandGlobalShortcutV1};
use protocol::hyprland_global_shortcuts_manager_v1::HyprlandGlobalShortcutsManagerV1;

pub const APP_ID: &str = "proscenio";

#[derive(Default)]
struct State {
    events: Vec<(String, bool)>,
}

impl Dispatch<HyprlandGlobalShortcutV1, String> for State {
    fn event(
        state: &mut State,
        _: &HyprlandGlobalShortcutV1,
        event: hyprland_global_shortcut_v1::Event,
        name: &String,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        match event {
            hyprland_global_shortcut_v1::Event::Pressed { .. } => {
                state.events.push((name.clone(), true));
            }
            hyprland_global_shortcut_v1::Event::Released { .. } => {
                state.events.push((name.clone(), false));
            }
        }
    }
}

delegate_noop!(State: ignore HyprlandGlobalShortcutsManagerV1);

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut State,
        _: &WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

pub struct Shortcuts {
    connection: Connection,
    queue: RefCell<wayland_client::EventQueue<State>>,
    state: RefCell<State>,
    manager: HyprlandGlobalShortcutsManagerV1,
    registrations: Vec<HyprlandGlobalShortcutV1>,
}

impl Shortcuts {
    pub fn publish(app_id: &str) {
        let Ok(connection) = Connection::connect_to_env() else {
            return;
        };
        let Ok((globals, queue)) = globals::registry_queue_init::<State>(&connection) else {
            return;
        };
        let Ok(manager) =
            globals.bind::<HyprlandGlobalShortcutsManagerV1, _, _>(&queue.handle(), 1..=1, ())
        else {
            return;
        };
        let registrations = actions::list()
            .into_iter()
            .map(|(name, description)| {
                manager.register_shortcut(
                    name.to_owned(),
                    app_id.to_owned(),
                    description.to_owned(),
                    String::new(),
                    &queue.handle(),
                    name.to_owned(),
                )
            })
            .collect();
        let _ = connection.flush();

        let shortcuts = Shortcuts {
            connection,
            queue: RefCell::new(queue),
            state: RefCell::new(State::default()),
            manager,
            registrations,
        };

        let fd = shortcuts.connection.backend().poll_fd().as_raw_fd();
        readable::add_watch(fd, move || shortcuts.pump());
    }

    fn pump(&self) -> glib::ControlFlow {
        let flow = readable::pump(
            &self.connection,
            &mut self.queue.borrow_mut(),
            &mut self.state.borrow_mut(),
        );
        let events: Vec<(String, bool)> = std::mem::take(&mut self.state.borrow_mut().events);
        for (name, is_pressed) in events {
            if is_pressed {
                actions::run(&name);
            } else {
                actions::release(&name);
            }
        }
        flow
    }
}

impl Drop for Shortcuts {
    fn drop(&mut self) {
        for shortcut in &self.registrations {
            shortcut.destroy();
        }
        self.manager.destroy();
        let _ = self.connection.flush();
    }
}
