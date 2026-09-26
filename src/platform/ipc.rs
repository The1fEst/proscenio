use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub const USAGE: &str =
    "usage: proscenio ipc show\n       proscenio ipc call <target> <function> [arguments…]\n";
const PATH: &str = "/dev/fEst/Proscenio";
const INTERFACE: &str = "dev.fEst.Proscenio.Ipc";
const TIMEOUT: i32 = 5000;
const NO_INSTANCE: u8 = 255;
const INTROSPECTION: &str = r#"<node>
  <interface name="dev.fEst.Proscenio.Ipc">
    <method name="Show">
      <arg name="targets" type="s" direction="out"/>
    </method>
    <method name="Call">
      <arg name="arguments" type="as" direction="in"/>
      <arg name="output" type="s" direction="out"/>
      <arg name="error" type="s" direction="out"/>
    </method>
  </interface>
</node>"#;

type Handler = Rc<dyn Fn(&[String]) -> Option<String>>;

struct Function {
    name: &'static str,
    parameters: &'static [&'static str],
    returns: &'static str,
    handler: Handler,
}

impl Function {
    fn definition(&self) -> String {
        format!(
            "function {}({}): {}",
            self.name,
            self.parameters.join(", "),
            self.returns
        )
    }
}

#[derive(Default)]
pub struct Ipc {
    targets: RefCell<Vec<(&'static str, Vec<Function>)>>,
}

impl Ipc {
    pub fn add(&self, target: &'static str, name: &'static str, action: impl Fn() + 'static) {
        self.add_with(target, name, &[], move |_| action());
    }

    pub fn add_with(
        &self,
        target: &'static str,
        name: &'static str,
        parameters: &'static [&'static str],
        action: impl Fn(&[String]) + 'static,
    ) {
        let function = Function {
            name,
            parameters,
            returns: "void",
            handler: Rc::new(move |arguments| {
                action(arguments);
                None
            }),
        };
        let mut targets = self.targets.borrow_mut();
        match targets.iter_mut().find(|(known, _)| *known == target) {
            Some((_, functions)) => {
                functions.retain(|known| known.name != name);
                functions.push(function);
            }
            None => targets.push((target, vec![function])),
        }
    }

    pub fn export(self: &Rc<Self>, connection: &gio::DBusConnection) {
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return;
        };
        let Some(interface) = node.lookup_interface(INTERFACE) else {
            return;
        };
        let ipc = Rc::downgrade(self);
        let _ = connection
            .register_object(PATH, &interface)
            .method_call(move |_, _, _, _, method, call, invocation| {
                let Some(ipc) = ipc.upgrade() else {
                    invocation.return_value(None);
                    return;
                };
                match method {
                    "Show" => invocation.return_value(Some(&(ipc.show(),).to_variant())),
                    "Call" => {
                        let arguments =
                            call.child_value(0).get::<Vec<String>>().unwrap_or_default();
                        let (output, error) = match ipc.call(&arguments) {
                            Ok(output) => (output, String::new()),
                            Err(error) => (String::new(), error),
                        };
                        invocation.return_value(Some(&(output, error).to_variant()));
                    }
                    _ => invocation.return_value(None),
                }
            })
            .build();
    }

    fn show(&self) -> String {
        let mut text = String::new();
        for (target, functions) in self.targets.borrow().iter() {
            text.push_str(&format!("target {target}\n"));
            for function in functions {
                text.push_str(&format!("  {}\n", function.definition()));
            }
        }
        text
    }

    fn call(&self, arguments: &[String]) -> Result<String, String> {
        let Some(target) = arguments.first() else {
            return Err("Target required to send message.".to_owned());
        };
        let Some(function) = arguments.get(1) else {
            return Err("Function required to send message.".to_owned());
        };
        let given = &arguments[2..];
        let handler = {
            let targets = self.targets.borrow();
            let Some((_, functions)) = targets.iter().find(|(name, _)| name == target) else {
                return Err("Target not found.".to_owned());
            };
            let Some(found) = functions
                .iter()
                .find(|candidate| candidate.name == function)
            else {
                return Err("Function not found.".to_owned());
            };
            let wanted = found.parameters.len();
            if given.len() != wanted {
                let amount = if given.len() > wanted { "many" } else { "few" };
                return Err(format!(
                    "Too {amount} arguments provided ({wanted} required but {} were provided.)\nFunction definition: {}",
                    given.len(),
                    found.definition()
                ));
            }
            found.handler.clone()
        };
        Ok(handler(given).unwrap_or_default())
    }
}

pub fn client(bus_name: &str, arguments: &[String]) -> glib::ExitCode {
    let (method, parameters) = match arguments.first().map(String::as_str) {
        Some("show") => ("Show", None),
        Some("call") => ("Call", Some((arguments[1..].to_vec(),).to_variant())),
        _ => {
            eprint!("{USAGE}");
            return glib::ExitCode::FAILURE;
        }
    };
    let reply =
        gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).and_then(|connection| {
            connection.call_sync(
                Some(bus_name),
                PATH,
                INTERFACE,
                method,
                parameters.as_ref(),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                TIMEOUT,
                gio::Cancellable::NONE,
            )
        });
    let Ok(reply) = reply else {
        eprintln!("No running instance of proscenio.");
        return glib::ExitCode::from(NO_INSTANCE);
    };
    let text = |index: usize| {
        reply
            .try_child_value(index)
            .and_then(|value| value.get::<String>())
    };
    if let Some(output) = text(0).filter(|output| !output.is_empty()) {
        print!("{}", with_newline(output));
    }
    if let Some(error) = text(1).filter(|error| !error.is_empty()) {
        eprint!("{}", with_newline(error));
    }
    glib::ExitCode::SUCCESS
}

fn with_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
