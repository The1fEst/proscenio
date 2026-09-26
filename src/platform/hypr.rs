use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};

pub fn socket_dir() -> Option<PathBuf> {
    let runtime = env::var_os("XDG_RUNTIME_DIR")?;
    let signature = env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    Some(PathBuf::from(runtime).join("hypr").join(signature))
}

pub fn request(command: &str) -> Option<String> {
    let path = socket_dir()?.join(".socket.sock");
    let mut stream = UnixStream::connect(path).ok()?;
    stream.write_all(command.as_bytes()).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    Some(reply)
}

pub fn focus_window(address: &str) {
    request(&format!(
        "dispatch hl.dsp.focus({{ window = \"address:{address}\" }})"
    ));
}

pub fn close_window(address: &str) {
    request(&format!(
        "dispatch hl.dsp.window.close({{ window = \"address:{address}\" }})"
    ));
}

pub fn focus_workspace(id: i32) {
    request(&format!("dispatch hl.dsp.focus({{ workspace = {id} }})"));
}

pub fn move_to_workspace(address: &str, workspace: i32) {
    request(&format!(
        "dispatch hl.dsp.window.move({{ workspace = {workspace}, follow = false, window = \"address:{address}\" }})"
    ));
}

pub fn move_window(address: &str, x: f64, y: f64) {
    request(&format!(
        "dispatch hl.dsp.window.move({{ x = \"{x}\", y = \"{y}\", window = \"address:{address}\" }})"
    ));
}

pub fn json(command: &str) -> Option<Value> {
    serde_json::from_str(&request(&format!("j/{command}"))?).ok()
}

pub fn option_int(name: &str) -> Option<i32> {
    let reply = request(&format!("getoption {name}"))?;
    let line = reply.lines().find(|line| line.starts_with("int: "))?;
    line[5..].trim().parse().ok()
}

pub fn focused_monitor() -> Option<String> {
    json("monitors")?
        .as_array()?
        .iter()
        .find(|entry| entry.get("focused").and_then(Value::as_bool) == Some(true))
        .and_then(|entry| entry.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub fn gaps_out_top() -> Option<i32> {
    let reply = request("getoption general:gaps_out")?;
    let line = reply.lines().find(|line| line.contains("gap data: "))?;
    let values = line.split("gap data: ").nth(1)?;
    values.split_whitespace().next()?.parse().ok()
}

#[derive(Clone, Default)]
pub struct Events {
    handlers: Rc<Listeners<(String, String)>>,
}

impl Events {
    pub fn subscribe(&self, handler: impl Fn(&str, &str) + 'static) -> Subscription {
        self.handlers
            .add_with(move |(event, data): &(String, String)| handler(event, data))
    }

    pub fn start(&self) {
        let Some(path) = socket_dir().map(|dir| dir.join(".socket2.sock")) else {
            return;
        };
        let address = gio::UnixSocketAddress::new(&path);
        let client = gio::SocketClient::new();
        let Ok(connection) =
            gio::prelude::SocketClientExt::connect(&client, &address, gio::Cancellable::NONE)
        else {
            return;
        };
        let reader = gio::DataInputStream::new(&connection.input_stream());
        let handlers = self.handlers.clone();
        glib::spawn_future_local(async move {
            let _connection = connection;
            while let Ok(Some(line)) = reader.read_line_utf8_future(glib::Priority::DEFAULT).await {
                let (event, data) = line.split_once(">>").unwrap_or((line.as_str(), ""));
                handlers.notify_with(&(event.to_owned(), data.to_owned()));
            }
        });
    }
}
