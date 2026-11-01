use std::rc::Rc;

use crate::platform::dbus::ipc::Ipc;
use crate::services::session::Session;

const KELVIN_RANGE: std::ops::RangeInclusive<i32> = 1000..=6500;

fn parse_kelvin(text: &str) -> Option<i32> {
    text.trim()
        .parse()
        .ok()
        .filter(|kelvin| KELVIN_RANGE.contains(kelvin))
}

pub fn register(ipc: &Ipc, session: &Rc<Session>) {
    ipc.add("nightLight", "toggle", {
        let session = session.clone();
        move || session.toggle_night_light()
    });
    ipc.add("nightLight", "enable", {
        let session = session.clone();
        move || session.toggle_temperature(Some(true))
    });
    ipc.add("nightLight", "disable", {
        let session = session.clone();
        move || session.toggle_temperature(Some(false))
    });
    ipc.add_with("nightLight", "setTemperature", &["kelvin: string"], {
        let session = session.clone();
        move |arguments| {
            if let Some(kelvin) = parse_kelvin(&arguments[0]) {
                session.set_temperature(kelvin);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_temperature_is_a_whole_number_from_1000_to_6500() {
        assert_eq!(parse_kelvin(" 4500 "), Some(4500));
        assert_eq!(parse_kelvin("1000"), Some(1000));
        assert_eq!(parse_kelvin("6500"), Some(6500));
        for text in ["999", "6501", "0", "-4500", "4500.5", "4500K", "warm", ""] {
            assert_eq!(parse_kelvin(text), None, "{text}");
        }
    }
}
