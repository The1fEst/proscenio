use crate::core::config;
use crate::core::watch::{self, Watch};
use crate::platform::hyprland::hyprlockclock;

const FORMAT_POINTER: &str = "/time/format";

pub struct HyprlockClockWriter {
    _format_watch: Watch,
}

impl HyprlockClockWriter {
    pub fn start() -> Self {
        write_clock();
        HyprlockClockWriter {
            _format_watch: watch::config(FORMAT_POINTER, write_clock),
        }
    }
}

fn write_clock() {
    let path = hyprlockclock::path();
    let is_twelve_hour = config::is_twelve_hour(&config::current().time_format);
    if let Err(error) = hyprlockclock::write(&path, is_twelve_hour) {
        eprintln!(
            "hyprlock clock: {} could not be written: {error}",
            path.display()
        );
    }
}
