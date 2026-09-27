use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::core::process;

pub struct Tool {
    pub program: &'static str,
    pub package: &'static str,
}

const fn tool(program: &'static str, package: &'static str) -> Tool {
    Tool { program, package }
}

pub const BLUETOOTHCTL: Tool = tool("bluetoothctl", "bluez-utils");
pub const CLIPHIST: Tool = tool("cliphist", "cliphist");
pub const GNOME_KEYRING: Tool = tool("gnome-keyring-daemon", "gnome-keyring");
pub const GRIM: Tool = tool("grim", "grim");
pub const HYPRIDLE: Tool = tool("hypridle", "hypridle");
pub const HYPRLOCK: Tool = tool("hyprlock", "hyprlock");
pub const HYPRPICKER: Tool = tool("hyprpicker", "hyprpicker");
pub const HYPRSUNSET: Tool = tool("hyprsunset", "hyprsunset");
pub const KDIALOG: Tool = tool("kdialog", "kdialog");
pub const MAGICK: Tool = tool("magick", "imagemagick");
pub const MATUGEN: Tool = tool("matugen", "matugen");
pub const MATERIAL_YOU: Tool = tool("kde-material-you-colors", "kde-material-you-colors");
pub const NMCLI: Tool = tool("nmcli", "networkmanager");
pub const PW_DUMP: Tool = tool("pw-dump", "pipewire");
pub const QALC: Tool = tool("qalc", "libqalculate");
pub const SATTY: Tool = tool("satty", "satty");
pub const SLURP: Tool = tool("slurp", "slurp");
pub const WF_RECORDER: Tool = tool("wf-recorder", "wf-recorder");
pub const WL_COPY: Tool = tool("wl-copy", "wl-clipboard");
pub const WPCTL: Tool = tool("wpctl", "wireplumber");
pub const YDOTOOL: Tool = tool("ydotool", "ydotool");

pub fn missing<'a>(tools: &[&'a Tool]) -> Vec<&'a Tool> {
    tools
        .iter()
        .copied()
        .filter(|tool| !process::exists(tool.program))
        .collect()
}

pub fn command_program(command: &str) -> Option<&str> {
    command.split_whitespace().find(|word| !word.contains('='))
}

pub fn command_missing(command: &str) -> Option<&str> {
    command_program(command).filter(|program| !process::exists(program))
}

fn listed(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

pub fn missing_message(missing: &[&Tool], effect: &str) -> String {
    let programs: Vec<&str> = missing.iter().map(|tool| tool.program).collect();
    let mut packages: Vec<&str> = Vec::new();
    for tool in missing {
        if !packages.contains(&tool.package) {
            packages.push(tool.package);
        }
    }
    let (verb, pronoun) = if programs.len() == 1 {
        ("is", "It comes")
    } else {
        ("are", "They come")
    };
    let noun = if packages.len() == 1 {
        "package"
    } else {
        "packages"
    };
    format!(
        "{} {verb} not installed, so {effect}. {pronoun} with the {} {noun}.",
        listed(&programs),
        listed(&packages)
    )
}

pub fn system_service(name: &str) -> bool {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) else {
        return false;
    };
    let call = |method: &str, arguments: Option<&glib::Variant>| {
        bus.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            method,
            arguments,
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .ok()
    };
    let owned = call("NameHasOwner", Some(&(name,).to_variant()))
        .and_then(|reply| reply.get::<(bool,)>())
        .is_some_and(|(owned,)| owned);
    owned
        || call("ListActivatableNames", None)
            .and_then(|reply| reply.get::<(Vec<String>,)>())
            .is_some_and(|(names,)| names.iter().any(|known| known == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_run_by_its_first_word_after_variables() {
        assert_eq!(command_program("kitty -1"), Some("kitty"));
        assert_eq!(command_program("GDK_SCALE=2 foot --hold"), Some("foot"));
        assert_eq!(command_program("  "), None);
    }

    #[test]
    fn missing_tools_are_named_with_their_packages() {
        assert_eq!(
            missing_message(&[&GRIM], "screenshots fail"),
            "grim is not installed, so screenshots fail. It comes with the grim package."
        );
        assert_eq!(
            missing_message(&[&GRIM, &SLURP, &WL_COPY], "capture fails"),
            "grim, slurp and wl-copy are not installed, so capture fails. They come with the grim, slurp and wl-clipboard packages."
        );
        assert_eq!(
            missing_message(&[&PW_DUMP, &tool("pw-play", "pipewire")], "nothing plays"),
            "pw-dump and pw-play are not installed, so nothing plays. They come with the pipewire package."
        );
    }
}
