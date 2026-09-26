use gtk4::gio;
use gtk4::glib;
use std::cell::{Ref, RefCell};
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::appearance::{self, Families, Font, Parts, State};

#[derive(Default)]
pub struct DesktopAppearance {
    state: RefCell<State>,
    families: RefCell<Families>,
    listeners: Listeners,
}

impl DesktopAppearance {
    pub fn new() -> Rc<Self> {
        let appearance = Rc::new(DesktopAppearance::default());
        appearance.reload();
        appearance
    }

    pub fn watch(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn state(&self) -> Ref<'_, State> {
        self.state.borrow()
    }

    pub fn font(&self, role: &str) -> Font {
        self.state
            .borrow()
            .fonts
            .get(role)
            .cloned()
            .unwrap_or_default()
    }

    pub fn family_names(&self) -> Vec<String> {
        self.families.borrow().keys().cloned().collect()
    }

    fn reload(self: &Rc<Self>) {
        let appearance = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok((state, families)) = gio::spawn_blocking(appearance::load).await else {
                return;
            };
            let Some(appearance) = appearance.upgrade() else {
                return;
            };
            appearance.state.replace(state);
            appearance.families.replace(families);
            appearance.listeners.notify();
        });
    }

    fn apply(self: &Rc<Self>, job: impl FnOnce() + Send + 'static) {
        let appearance = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(job).await;
            if let Some(appearance) = appearance.upgrade() {
                appearance.reload();
            }
        });
    }

    pub fn set_cursor(self: &Rc<Self>, theme: &str, size: i64) {
        let theme = theme.to_owned();
        self.apply(move || appearance::set_cursor(&theme, size));
    }

    pub fn set_icons(self: &Rc<Self>, theme: &str) {
        let theme = theme.to_owned();
        self.apply(move || appearance::set_icons(&theme));
    }

    pub fn set_font(self: &Rc<Self>, role: &'static str, parts: Parts) {
        if parts.family.is_none() && parts.style.is_none() && parts.size.is_none() {
            return;
        }
        self.apply(move || appearance::set_fonts(role, &parts));
    }

    pub fn set_themes(self: &Rc<Self>, gtk: &str, qt: &str) {
        let (gtk, qt) = (gtk.to_owned(), qt.to_owned());
        self.apply(move || appearance::set_themes(&gtk, &qt));
    }
}
