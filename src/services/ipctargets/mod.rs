mod brightness;
mod nightlight;
mod notifications;

use crate::platform::dbus::ipc::Ipc;
use crate::services::Services;

pub fn register(ipc: &Ipc, services: &Services) {
    notifications::register(ipc, &services.notifications);
    nightlight::register(ipc, &services.session);
    brightness::register(ipc, &services.light);
}

fn parse_percent(text: &str) -> Option<u8> {
    text.trim().parse().ok().filter(|percent| *percent <= 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percent_is_a_whole_number_from_0_to_100() {
        assert_eq!(parse_percent(" 40 "), Some(40));
        assert_eq!(parse_percent("0"), Some(0));
        assert_eq!(parse_percent("100"), Some(100));
        for text in ["101", "-1", "40.5", "40%", "half", "256", ""] {
            assert_eq!(parse_percent(text), None, "{text}");
        }
    }
}
