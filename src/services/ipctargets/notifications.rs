use crate::platform::dbus::ipc::Ipc;
use crate::services::overlay::notifications::Notifications;

pub fn register(ipc: &Ipc, notifications: &Notifications) {
    ipc.add("notifications", "dismissAll", {
        let notifications = notifications.clone();
        move || notifications.discard_all()
    });
    ipc.add("notifications", "dismissLatest", {
        let notifications = notifications.clone();
        move || {
            if let Some(id) = notifications.latest_popup_id() {
                notifications.dismiss(id);
            }
        }
    });
    ipc.add("notifications", "hidePopups", {
        let notifications = notifications.clone();
        move || notifications.timeout_all()
    });
}
