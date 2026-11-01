mod notifications;

use crate::platform::dbus::ipc::Ipc;
use crate::services::Services;

pub fn register(ipc: &Ipc, services: &Services) {
    notifications::register(ipc, &services.notifications);
}
