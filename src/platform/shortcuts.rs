use gtk4::glib;
use std::cell::RefCell;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, globals};

use crate::core::actions;
use crate::platform::readable;

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
    fired: Vec<(String, bool)>,
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
                state.fired.push((name.clone(), true));
            }
            hyprland_global_shortcut_v1::Event::Released { .. } => {
                state.fired.push((name.clone(), false));
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
    held: Vec<HyprlandGlobalShortcutV1>,
}

impl Shortcuts {
    pub fn publish(app_id: &str) -> Option<Rc<Self>> {
        let connection = Connection::connect_to_env().ok()?;
        let (globals, queue) = globals::registry_queue_init::<State>(&connection).ok()?;
        let manager: HyprlandGlobalShortcutsManagerV1 =
            globals.bind(&queue.handle(), 1..=1, ()).ok()?;
        let held = actions::list()
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

        let shortcuts = Rc::new(Shortcuts {
            connection,
            queue: RefCell::new(queue),
            state: RefCell::new(State::default()),
            manager,
            held,
        });

        let again = shortcuts.clone();
        let fd = shortcuts.connection.backend().poll_fd().as_raw_fd();
        readable::when_readable(fd, move || {
            again.pump();
            glib::ControlFlow::Continue
        });
        Some(shortcuts)
    }

    fn pump(&self) {
        {
            let mut queue = self.queue.borrow_mut();
            let mut state = self.state.borrow_mut();
            let _ = queue.dispatch_pending(&mut state);
            if let Some(guard) = queue.prepare_read() {
                let _ = guard.read();
            }
            let _ = queue.dispatch_pending(&mut state);
        }
        let fired: Vec<(String, bool)> = std::mem::take(&mut self.state.borrow_mut().fired);
        for (name, pressed) in fired {
            if pressed {
                actions::run(&name);
            } else {
                actions::release(&name);
            }
        }
    }
}

impl Drop for Shortcuts {
    fn drop(&mut self) {
        for shortcut in &self.held {
            shortcut.destroy();
        }
        self.manager.destroy();
        let _ = self.connection.flush();
    }
}
