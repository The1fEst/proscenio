use super::parse_percent;
use crate::platform::dbus::ipc::Ipc;
use crate::platform::hyprland::hypr;
use crate::services::hardware::brightness::Light;

pub fn register(ipc: &Ipc, light: &Light) {
    ipc.add_with("brightness", "set", &["percent: string"], {
        let light = light.clone();
        move |arguments| {
            let Some(percent) = parse_percent(&arguments[0]) else {
                return;
            };
            if let Some(name) = hypr::focused_monitor() {
                light.set_level(&name, f64::from(percent) / 100.0);
            }
        }
    });
}
