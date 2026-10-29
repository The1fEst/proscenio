use gtk4::glib;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::hyprland::hypr::Events;

const NOTIFY_DELAY: Duration = Duration::from_millis(150);

#[derive(Clone)]
pub struct UrgentWindows {
    addresses: Rc<RefCell<HashSet<String>>>,
    listeners: Rc<Listeners<String>>,
}

impl UrgentWindows {
    pub fn new(events: &Events) -> Self {
        let urgent_windows = UrgentWindows {
            addresses: Rc::default(),
            listeners: Rc::default(),
        };
        events
            .subscribe({
                let urgent_windows = urgent_windows.clone();
                move |event, data| urgent_windows.handle(event, data)
            })
            .detach();
        urgent_windows
    }

    fn handle(&self, event: &str, data: &str) {
        let new_address = apply_event(&mut self.addresses.borrow_mut(), event, data);
        let Some(address) = new_address else {
            return;
        };
        let urgent_windows = self.clone();
        glib::spawn_future_local(async move {
            glib::timeout_future(NOTIFY_DELAY).await;
            if urgent_windows.contains(&address) {
                urgent_windows.listeners.notify_with(&address);
            }
        });
    }

    pub fn contains(&self, address: &str) -> bool {
        self.addresses.borrow().contains(address)
    }

    pub fn subscribe(&self, listener: impl Fn(&str) + 'static) -> Subscription {
        self.listeners
            .add_with(move |address: &String| listener(address))
    }
}

fn apply_event(addresses: &mut HashSet<String>, event: &str, data: &str) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    let address = format!("0x{}", data.trim_start_matches("0x"));
    match event {
        "urgent" => addresses.insert(address.clone()).then_some(address),
        "activewindowv2" | "closewindow" => {
            addresses.remove(&address);
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_window_stays_urgent_until_it_is_focused_or_closed() {
        let mut addresses = HashSet::new();
        assert_eq!(
            apply_event(&mut addresses, "urgent", "5588a1"),
            Some("0x5588a1".to_owned())
        );
        assert_eq!(apply_event(&mut addresses, "urgent", "5588a1"), None);
        assert!(addresses.contains("0x5588a1"));

        apply_event(&mut addresses, "activewindowv2", "");
        apply_event(&mut addresses, "activewindowv2", "7f00");
        assert!(addresses.contains("0x5588a1"));

        apply_event(&mut addresses, "activewindowv2", "5588a1");
        assert!(addresses.is_empty());

        apply_event(&mut addresses, "urgent", "5588a1");
        apply_event(&mut addresses, "urgent", "7f00");
        apply_event(&mut addresses, "closewindow", "5588a1");
        assert_eq!(addresses, HashSet::from(["0x7f00".to_owned()]));
    }

    #[test]
    fn listeners_hear_only_of_windows_that_are_still_urgent_after_a_moment() {
        let context = glib::MainContext::new();
        let notified_addresses = context
            .with_thread_default(|| {
                let urgent_windows = UrgentWindows::new(&Events::default());
                let notified_addresses = Rc::new(RefCell::new(Vec::new()));
                let _subscription = urgent_windows.subscribe({
                    let notified_addresses = notified_addresses.clone();
                    move |address| notified_addresses.borrow_mut().push(address.to_owned())
                });
                urgent_windows.handle("urgent", "1");
                urgent_windows.handle("activewindowv2", "1");
                urgent_windows.handle("urgent", "2");
                let deadline = Instant::now() + Duration::from_secs(5);
                while notified_addresses.borrow().is_empty() && Instant::now() < deadline {
                    context.iteration(true);
                }
                notified_addresses.take()
            })
            .unwrap();
        assert_eq!(notified_addresses, ["0x2"]);
    }
}
