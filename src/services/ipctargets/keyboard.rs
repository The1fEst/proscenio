use crate::platform::dbus::ipc::Ipc;
use crate::services::hardware::xkb::Xkb;

pub fn register(ipc: &Ipc, xkb: &Xkb) {
    ipc.add("keyboard", "cycleLayout", {
        let xkb = xkb.clone();
        move || xkb.cycle_layout()
    });
}
