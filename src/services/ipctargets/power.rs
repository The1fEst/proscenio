use crate::platform::dbus::ipc::Ipc;
use crate::services::hardware::power::Power;

pub fn register(ipc: &Ipc, power: &Power) {
    ipc.add("powerProfile", "cycle", {
        let power = power.clone();
        move || power.cycle()
    });
}
