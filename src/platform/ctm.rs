use gtk4::glib;
use std::cell::RefCell;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use wayland_client::globals::{GlobalList, GlobalListContents};
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, globals};

use crate::platform::readable;

pub mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/hyprland-ctm-control-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/hyprland-ctm-control-v1.xml");
}

use protocol::hyprland_ctm_control_manager_v1::{self, HyprlandCtmControlManagerV1};

const OUTPUT_VERSION: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Outputs,
    Blocked,
}

struct Output {
    global: u32,
    output: WlOutput,
    name: Option<String>,
}

#[derive(Default)]
struct State {
    outputs: Vec<Output>,
    changes: Vec<Change>,
}

impl State {
    fn add(
        &mut self,
        registry: &WlRegistry,
        global: u32,
        version: u32,
        queue: &QueueHandle<State>,
    ) {
        let output = registry.bind::<WlOutput, u32, State>(
            global,
            version.min(OUTPUT_VERSION),
            queue,
            global,
        );
        self.outputs.push(Output {
            global,
            output,
            name: None,
        });
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        state: &mut State,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        queue: &QueueHandle<State>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == WlOutput::interface().name => {
                state.add(registry, name, version, queue);
            }
            wl_registry::Event::GlobalRemove { name } => {
                let before = state.outputs.len();
                state.outputs.retain(|output| {
                    let gone = output.global == name;
                    if gone && output.output.version() >= 3 {
                        output.output.release();
                    }
                    !gone
                });
                if state.outputs.len() != before {
                    state.changes.push(Change::Outputs);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlOutput, u32> for State {
    fn event(
        state: &mut State,
        _: &WlOutput,
        event: wl_output::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        match event {
            wl_output::Event::Name { name } => {
                if let Some(output) = state
                    .outputs
                    .iter_mut()
                    .find(|output| output.global == *global)
                {
                    output.name = Some(name);
                }
            }
            wl_output::Event::Done => state.changes.push(Change::Outputs),
            _ => {}
        }
    }
}

impl Dispatch<HyprlandCtmControlManagerV1, ()> for State {
    fn event(
        state: &mut State,
        _: &HyprlandCtmControlManagerV1,
        event: hyprland_ctm_control_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        match event {
            hyprland_ctm_control_manager_v1::Event::Blocked => state.changes.push(Change::Blocked),
        }
    }
}

pub struct CtmControl {
    connection: Connection,
    globals: GlobalList,
    queue: RefCell<wayland_client::EventQueue<State>>,
    state: RefCell<State>,
    manager: RefCell<Option<HyprlandCtmControlManagerV1>>,
    changed: Box<dyn Fn(Change)>,
}

impl CtmControl {
    pub fn connect(changed: impl Fn(Change) + 'static) -> Option<Rc<Self>> {
        let connection = Connection::connect_to_env().ok()?;
        let (globals, queue) = globals::registry_queue_init::<State>(&connection).ok()?;
        let manager: HyprlandCtmControlManagerV1 = globals.bind(&queue.handle(), 1..=2, ()).ok()?;
        let mut state = State::default();
        globals.contents().with_list(|list| {
            for global in list {
                if global.interface == WlOutput::interface().name {
                    state.add(
                        globals.registry(),
                        global.name,
                        global.version,
                        &queue.handle(),
                    );
                }
            }
        });
        let _ = connection.flush();

        let control = Rc::new(CtmControl {
            connection,
            globals,
            queue: RefCell::new(queue),
            state: RefCell::new(state),
            manager: RefCell::new(Some(manager)),
            changed: Box::new(changed),
        });

        let weak = Rc::downgrade(&control);
        let fd = control.connection.backend().poll_fd().as_raw_fd();
        readable::when_readable(fd, move || {
            let Some(control) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            control.pump();
            glib::ControlFlow::Continue
        });
        Some(control)
    }

    pub fn set(&self, scale: impl Fn(&str) -> [f64; 3]) {
        let manager = self.manager.borrow();
        let Some(manager) = manager.as_ref() else {
            return;
        };
        for output in &self.state.borrow().outputs {
            let Some(name) = &output.name else {
                continue;
            };
            let [red, green, blue] = scale(name);
            manager.set_ctm_for_output(
                &output.output,
                red,
                0.0,
                0.0,
                0.0,
                green,
                0.0,
                0.0,
                0.0,
                blue,
            );
        }
        manager.commit();
        let _ = self.connection.flush();
    }

    pub fn rebind(&self) {
        let manager = self
            .globals
            .bind(&self.queue.borrow().handle(), 1..=2, ())
            .ok();
        if let Some(old) = self.manager.replace(manager) {
            old.destroy();
        }
        let _ = self.connection.flush();
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
        let mut changes = std::mem::take(&mut self.state.borrow_mut().changes);
        changes.dedup();
        for change in changes {
            if change == Change::Blocked
                && let Some(manager) = self.manager.take()
            {
                manager.destroy();
                let _ = self.connection.flush();
            }
            (self.changed)(change);
        }
    }
}

impl Drop for CtmControl {
    fn drop(&mut self) {
        if let Some(manager) = self.manager.take() {
            manager.destroy();
        }
        for output in &self.state.borrow().outputs {
            if output.output.version() >= 3 {
                output.output.release();
            }
        }
        let _ = self.connection.flush();
    }
}
