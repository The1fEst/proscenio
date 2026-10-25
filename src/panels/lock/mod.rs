mod keyring;
mod surfaces;

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::Listeners;
use crate::core::{config, persistent};
use crate::platform::dbus::fprint::Reader;
use crate::platform::dbus::sleep::SleepWatcher;
use crate::platform::hyprland::hypr;
use crate::platform::security::pam;
use crate::platform::wayland::locknotify::LockNotifier;
use crate::platform::wayland::sessionlock::SessionLock;
use crate::services::Services;
use crate::ui::theme::SharedTheme;
use keyring::unlock_keyring;
use surfaces::Surface;

const PAM_SERVICE: &str = "login";
const CLEAR_AFTER: Duration = Duration::from_secs(10);
const RESTORE_DELAY: Duration = Duration::from_millis(150);
const GRANT_TIMEOUT: Duration = Duration::from_secs(6);
const LOCK_WAIT: Duration = Duration::from_millis(500);
const SLEEP_DELAY_LIMIT: Duration = Duration::from_secs(2);
const DRAW_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REPLACEMENTS: u32 = 2;
const STALL_LIMIT: Duration = Duration::from_secs(1);
const HIDDEN_WORKSPACE: i64 = 2_147_483_647;

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Unlock,
    Poweroff,
    Reboot,
}

#[derive(Default)]
struct Context {
    text: RefCell<String>,
    is_in_progress: Cell<bool>,
    has_failed: Cell<bool>,
    is_fingerprint_active: Cell<bool>,
    target: Cell<Option<Target>>,
    should_also_inhibit: Cell<bool>,
    is_ctrl_held: Cell<bool>,
    clear_timer: Cell<Option<glib::SourceId>>,
    changed: Listeners,
    shake_requested: Listeners,
    focus_requested: Listeners,
}

impl Context {
    fn target(&self) -> Target {
        self.target.get().unwrap_or(Target::Unlock)
    }

    fn set_text(self: &Rc<Self>, password_text: &str) {
        if *self.text.borrow() == password_text {
            return;
        }
        self.replace_text(password_text.to_owned());
        if !password_text.is_empty() {
            self.has_failed.set(false);
        }
        self.restart_clear();
        self.changed.notify();
    }

    fn restart_clear(self: &Rc<Self>) {
        if let Some(source) = self.clear_timer.take() {
            source.remove();
        }
        let context = Rc::downgrade(self);
        self.clear_timer
            .set(Some(glib::timeout_add_local_once(CLEAR_AFTER, move || {
                if let Some(context) = context.upgrade() {
                    context.clear_timer.set(None);
                    if !context.try_expire() {
                        context.restart_clear();
                    }
                }
            })));
    }

    fn try_expire(self: &Rc<Self>) -> bool {
        if self.is_in_progress.get() {
            return false;
        }
        self.reset();
        true
    }

    fn reset(self: &Rc<Self>) {
        self.target.set(None);
        self.is_in_progress.set(false);
        self.replace_text(String::new());
        self.changed.notify();
    }

    fn replace_text(&self, password_text: String) {
        let previous_text = self.text.replace(password_text);
        pam::wipe(&mut previous_text.into_bytes());
    }
}

pub struct Lock {
    theme: SharedTheme,
    services: Rc<Services>,
    instance: RefCell<Option<Rc<SessionLock>>>,
    context: Rc<Context>,
    surfaces: RefCell<Vec<Rc<Surface>>>,
    retired_surfaces: RefCell<Vec<Rc<Surface>>>,
    replacement_counts: RefCell<HashMap<String, u32>>,
    workspaces_to_restore: RefCell<Vec<(String, i64)>>,
    notifier: RefCell<Option<Rc<LockNotifier>>>,
    reader: RefCell<Option<Rc<Reader>>>,
    is_session_locked: Cell<bool>,
    is_granted: Cell<bool>,
    has_seen_session_lock: Cell<bool>,
    grant_timer: RefCell<Option<glib::SourceId>>,
    sleep_watcher: RefCell<Option<Rc<SleepWatcher>>>,
    sleep_timer: RefCell<Option<glib::SourceId>>,
}

#[derive(Debug, PartialEq)]
enum StallAction {
    Keep,
    Replace,
    GiveUp,
}

fn choose_stall_action(stall: Duration, replacement_count: u32) -> StallAction {
    if stall < STALL_LIMIT {
        return StallAction::Keep;
    }
    if replacement_count >= MAX_REPLACEMENTS {
        return StallAction::GiveUp;
    }
    StallAction::Replace
}

fn is_ready_for_sleep(is_granted: bool, settled_flags: &[bool]) -> bool {
    is_granted && !settled_flags.is_empty() && settled_flags.iter().all(|is_settled| *is_settled)
}

impl Lock {
    pub fn new(theme: &SharedTheme, services: &Rc<Services>) -> Rc<Self> {
        let lock = Rc::new(Lock {
            theme: theme.clone(),
            services: services.clone(),
            instance: RefCell::new(None),
            context: Rc::new(Context::default()),
            surfaces: RefCell::new(Vec::new()),
            retired_surfaces: RefCell::new(Vec::new()),
            replacement_counts: RefCell::new(HashMap::new()),
            workspaces_to_restore: RefCell::new(Vec::new()),
            notifier: RefCell::new(None),
            reader: RefCell::new(None),
            is_session_locked: Cell::new(false),
            is_granted: Cell::new(false),
            has_seen_session_lock: Cell::new(false),
            grant_timer: RefCell::new(None),
            sleep_watcher: RefCell::new(None),
            sleep_timer: RefCell::new(None),
        });
        let notifier = LockNotifier::watch(
            {
                let lock = Rc::downgrade(&lock);
                move |is_locked| {
                    if let Some(lock) = lock.upgrade() {
                        lock.on_session_changed(is_locked);
                    }
                }
            },
            {
                let lock = Rc::downgrade(&lock);
                move || {
                    if let Some(lock) = lock.upgrade() {
                        lock.notifier.take();
                    }
                }
            },
        );
        lock.notifier.replace(notifier);
        let weak = Rc::downgrade(&lock);
        glib::spawn_future_local(async move {
            let Ok(system) = gio::bus_get_future(gio::BusType::System).await else {
                return;
            };
            let Some(lock) = weak.upgrade() else {
                return;
            };
            let watcher = SleepWatcher::watch(&system, "Show the lock screen before sleep", {
                let lock = Rc::downgrade(&lock);
                move |is_sleeping| {
                    if let Some(lock) = lock.upgrade() {
                        lock.on_sleep_changed(is_sleeping);
                    }
                }
            });
            lock.sleep_watcher.replace(Some(watcher));
        });
        lock
    }

    fn on_sleep_changed(self: &Rc<Self>, is_sleeping: bool) {
        if let Some(timer) = self.sleep_timer.take() {
            timer.remove();
        }
        if !is_sleeping {
            self.renew_surfaces();
            return;
        }
        self.start_sleep_timer(LOCK_WAIT, |lock| {
            if lock.instance.borrow().is_none() {
                lock.release_sleep();
                return;
            }
            lock.start_sleep_timer(SLEEP_DELAY_LIMIT - LOCK_WAIT, |lock| lock.release_sleep());
        });
        self.check_sleep();
    }

    fn start_sleep_timer(
        self: &Rc<Self>,
        delay: Duration,
        action: impl FnOnce(&Rc<Self>) + 'static,
    ) {
        let lock = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(delay, move || {
            if let Some(lock) = lock.upgrade() {
                lock.sleep_timer.take();
                action(&lock);
            }
        });
        self.sleep_timer.replace(Some(timer));
    }

    fn check_sleep(&self) {
        if self.sleep_timer.borrow().is_none() {
            return;
        }
        let settled_flags: Vec<bool> = self
            .surfaces
            .borrow()
            .iter()
            .map(|surface| surface.is_settled.get())
            .collect();
        if is_ready_for_sleep(self.is_granted.get(), &settled_flags) {
            self.release_sleep();
        }
    }

    fn release_sleep(&self) {
        if let Some(timer) = self.sleep_timer.take() {
            timer.remove();
        }
        if let Some(watcher) = self.sleep_watcher.borrow().as_ref() {
            watcher.release();
        }
    }

    fn renew_surfaces(self: &Rc<Self>) {
        if !self.is_granted.get() {
            return;
        }
        self.replacement_counts.borrow_mut().clear();
        let surfaces = self.surfaces.take();
        for surface in &surfaces {
            surface.window.destroy();
        }
        self.retired_surfaces.borrow_mut().extend(surfaces);
        let monitors: Vec<gdk::Monitor> = gdk::Display::default()
            .map(|display| {
                display
                    .monitors()
                    .iter::<gdk::Monitor>()
                    .flatten()
                    .collect()
            })
            .unwrap_or_default();
        for monitor in monitors {
            self.cover(&monitor);
        }
    }

    fn retire(&self, surface: &Rc<Surface>) {
        self.surfaces
            .borrow_mut()
            .retain(|tracked_surface| !Rc::ptr_eq(tracked_surface, surface));
        surface.window.destroy();
        self.retired_surfaces.borrow_mut().push(surface.clone());
    }

    fn release_surface(&self, surface: &Rc<Surface>) {
        let mut released_surfaces = Vec::new();
        for surfaces in [&self.surfaces, &self.retired_surfaces] {
            released_surfaces.extend(
                surfaces
                    .borrow_mut()
                    .extract_if(.., |listed_surface| Rc::ptr_eq(listed_surface, surface)),
            );
        }
        if released_surfaces.is_empty() {
            return;
        }
        self.replacement_counts
            .borrow_mut()
            .remove(&crate::connector(&surface.monitor));
        self.check_sleep();
    }

    fn is_tracked(&self, surface: &Rc<Surface>) -> bool {
        self.surfaces
            .borrow()
            .iter()
            .any(|tracked_surface| Rc::ptr_eq(tracked_surface, surface))
    }

    fn watch_frames(self: &Rc<Self>, surface: &Rc<Surface>) {
        let last_frame_time = Rc::new(Cell::new(glib::monotonic_time()));
        let tick_callback = surface.window.add_tick_callback({
            let last_frame_time = last_frame_time.clone();
            move |_, _| {
                last_frame_time.set(glib::monotonic_time());
                glib::ControlFlow::Continue
            }
        });
        let lock = Rc::downgrade(self);
        let weak_surface = Rc::downgrade(surface);
        glib::timeout_add_local_once(DRAW_TIMEOUT, move || {
            tick_callback.remove();
            let stall_micros = glib::monotonic_time() - last_frame_time.get();
            let stall = Duration::from_micros(stall_micros.max(0) as u64);
            if let (Some(lock), Some(surface)) = (lock.upgrade(), weak_surface.upgrade()) {
                lock.replace_if_stalled(&surface, stall);
            }
        });
    }

    fn replace_if_stalled(self: &Rc<Self>, surface: &Rc<Surface>, stall: Duration) {
        if !self.is_granted.get() || !self.is_tracked(surface) {
            return;
        }
        let connector = crate::connector(&surface.monitor);
        let replacement_count = self
            .replacement_counts
            .borrow()
            .get(&connector)
            .copied()
            .unwrap_or(0);
        match choose_stall_action(stall, replacement_count) {
            StallAction::Keep => {}
            StallAction::GiveUp => eprintln!(
                "lock: the lock surface on {connector} has had no frame for {} ms, and its {MAX_REPLACEMENTS} replacements are used up",
                stall.as_millis()
            ),
            StallAction::Replace => {
                eprintln!(
                    "lock: the lock surface on {connector} has had no frame for {} ms, replacing it ({} of {MAX_REPLACEMENTS})",
                    stall.as_millis(),
                    replacement_count + 1
                );
                self.replacement_counts
                    .borrow_mut()
                    .insert(connector, replacement_count + 1);
                self.retire(surface);
                self.cover(&surface.monitor);
                self.check_sleep();
            }
        }
    }

    pub fn lock(self: &Rc<Self>) {
        if config::value_bool("/lock/useHyprlock", false)
            && crate::core::process::exists("hyprlock")
        {
            crate::platform::system::desktop::run_shell("pidof hyprlock || hyprlock");
            return;
        }
        if self.instance.borrow().is_some() {
            return;
        }
        self.context.reset();
        let instance = Rc::new(SessionLock::new());
        instance.connect_monitor({
            let lock = Rc::downgrade(self);
            move |monitor| {
                if let Some(lock) = lock.upgrade() {
                    lock.cover(monitor);
                }
            }
        });
        instance.connect_locked({
            let lock = Rc::downgrade(self);
            move || {
                if let Some(lock) = lock.upgrade() {
                    lock.on_granted();
                }
            }
        });
        instance.connect_unlocked({
            let lock = Rc::downgrade(self);
            move || {
                if let Some(lock) = lock.upgrade() {
                    lock.on_released();
                }
            }
        });
        instance.connect_failed({
            let lock = Rc::downgrade(self);
            move || {
                if let Some(lock) = lock.upgrade() {
                    lock.on_released();
                }
            }
        });
        self.instance.replace(Some(instance.clone()));
        self.is_granted.set(false);
        self.has_seen_session_lock.set(false);
        self.services.states.set_screen_locked(true);
        instance.lock();
        if self.instance.borrow().is_none() {
            return;
        }
        self.hide_windows();
        self.watch_finger();
        let grant_timer = glib::timeout_add_local_once(GRANT_TIMEOUT, {
            let lock = Rc::downgrade(self);
            move || {
                if let Some(lock) = lock.upgrade() {
                    lock.grant_timer.take();
                    if !lock.is_granted.get() {
                        lock.stop_finger();
                        lock.restore_desktop();
                    }
                }
            }
        });
        self.grant_timer.replace(Some(grant_timer));
    }

    fn watch_finger(self: &Rc<Self>) {
        let lock = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(system) = gio::bus_get_future(gio::BusType::System).await else {
                return;
            };
            let user = glib::user_name().to_string_lossy().into_owned();
            let Some(reader) = Reader::open(&system, &user).await else {
                return;
            };
            let Some(lock) = lock
                .upgrade()
                .filter(|lock| lock.instance.borrow().is_some())
            else {
                return;
            };
            if let Some(previous) = lock.reader.replace(Some(reader.clone())) {
                previous.stop();
            }
            lock.context.is_fingerprint_active.set(true);
            lock.context.changed.notify();
            let lock = Rc::downgrade(&lock);
            reader
                .verify(move || {
                    if let Some(lock) = lock.upgrade() {
                        lock.run_target(lock.context.target(), None);
                    }
                })
                .await;
        });
    }

    fn on_granted(self: &Rc<Self>) {
        if let Some(grant_timer) = self.grant_timer.take() {
            grant_timer.remove();
        }
        let notifier = self.notifier.borrow().clone();
        if let Some(notifier) = notifier {
            notifier.pump();
        }
        self.is_granted.set(true);
        self.has_seen_session_lock.set(self.is_session_locked.get());
        if !self.services.states.is_screen_locked.get() {
            self.services.states.set_screen_locked(true);
            self.hide_windows();
            self.watch_finger();
        }
        let surfaces = self.surfaces.borrow().clone();
        for surface in &surfaces {
            self.watch_frames(surface);
        }
        self.check_sleep();
    }

    fn on_session_changed(self: &Rc<Self>, is_locked: bool) {
        self.is_session_locked.set(is_locked);
        if !self.is_granted.get() {
            return;
        }
        if is_locked {
            self.has_seen_session_lock.set(true);
            return;
        }
        if !self.has_seen_session_lock.get() {
            return;
        }
        let instance = self.instance.borrow().clone();
        match instance {
            Some(instance) => instance.unlock(),
            None => self.on_released(),
        }
    }

    pub fn focus(&self) {
        self.context.focus_requested.notify();
    }

    pub fn start(self: &Rc<Self>) {
        let is_new_instance = persistent::is_new_hyprland_instance();
        if is_new_instance && config::value_bool("/lock/launchOnStartup", false) {
            self.lock();
        }
    }

    fn hide_windows(&self) {
        let monitors = hypr::json("monitors")
            .and_then(|monitors| monitors.as_array().cloned())
            .unwrap_or_default();
        let mut workspaces = Vec::new();
        for monitor in &monitors {
            let Some(name) = monitor.get("name").and_then(|name| name.as_str()) else {
                return;
            };
            let Some(workspace) = monitor
                .get("activeWorkspace")
                .and_then(|workspace| workspace.get("id"))
                .and_then(|id| id.as_i64())
            else {
                return;
            };
            workspaces.push((name.to_owned(), workspace));
        }
        for (name, workspace) in &workspaces {
            hypr::request(&format!("dispatch hl.dsp.focus({{monitor=\"{name}\"}})"));
            hypr::request(&format!(
                "dispatch hl.dsp.focus({{workspace={}}})",
                HIDDEN_WORKSPACE - workspace
            ));
        }
        self.workspaces_to_restore.replace(workspaces);
    }

    fn restore_windows(&self) {
        let workspaces = self.workspaces_to_restore.take();
        glib::timeout_add_local_once(RESTORE_DELAY, move || {
            for (name, workspace) in workspaces {
                hypr::request(&format!("dispatch hl.dsp.focus({{monitor=\"{name}\"}})"));
                hypr::request(&format!("dispatch hl.dsp.focus({{workspace={workspace}}})"));
            }
        });
    }

    fn on_released(self: &Rc<Self>) {
        if let Some(grant_timer) = self.grant_timer.take() {
            grant_timer.remove();
        }
        self.is_granted.set(false);
        self.has_seen_session_lock.set(false);
        self.instance.replace(None);
        self.stop_finger();
        self.surfaces.borrow_mut().clear();
        self.replacement_counts.borrow_mut().clear();
        let retired_surfaces = self.retired_surfaces.take();
        glib::idle_add_local_once(move || drop(retired_surfaces));
        self.restore_desktop();
    }

    fn stop_finger(&self) {
        if let Some(reader) = self.reader.take() {
            reader.stop();
        }
        self.context.is_fingerprint_active.set(false);
    }

    fn restore_desktop(&self) {
        if self.services.states.is_screen_locked.get() {
            self.services.states.set_screen_locked(false);
            self.restore_windows();
        }
    }

    fn try_unlock(self: &Rc<Self>, should_also_inhibit: bool) {
        if self.context.is_in_progress.replace(true) {
            return;
        }
        self.context.should_also_inhibit.set(should_also_inhibit);
        self.context.changed.notify();
        let password = self.context.text.borrow().clone();
        let unlock_password = password.clone();
        let target = self.context.target();
        let user = glib::user_name().to_string_lossy().into_owned();
        let lock = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let auth_result =
                gio::spawn_blocking(move || pam::authenticate(PAM_SERVICE, &user, password)).await;
            let Some(lock) = lock.upgrade() else {
                pam::wipe(&mut unlock_password.into_bytes());
                return;
            };
            if auth_result.unwrap_or(false) {
                lock.run_target(target, Some(unlock_password));
                return;
            }
            pam::wipe(&mut unlock_password.into_bytes());
            lock.context.replace_text(String::new());
            lock.context.is_in_progress.set(false);
            lock.context.has_failed.set(true);
            lock.context.changed.notify();
            lock.context.shake_requested.notify();
        });
    }

    fn run_target(self: &Rc<Self>, target: Target, password: Option<String>) {
        if let Some(password) = password {
            if target == Target::Unlock
                && !password.is_empty()
                && config::value_bool("/lock/security/unlockKeyring", true)
            {
                self.services.prompter.answer_with_login_password(&password);
                gio::spawn_blocking(move || unlock_keyring(password));
            } else {
                pam::wipe(&mut password.into_bytes());
            }
        }
        match target {
            Target::Poweroff => {
                self.context.reset();
                self.services.session.poweroff();
                return;
            }
            Target::Reboot => {
                self.context.reset();
                self.services.session.reboot();
                return;
            }
            Target::Unlock => {}
        }
        let instance = self.instance.borrow().clone();
        if let Some(instance) = instance {
            instance.unlock();
        }
        self.context.reset();
        if self.context.should_also_inhibit.replace(false)
            && !self.services.session.keeps_awake.get()
        {
            self.services.session.toggle_keep_awake();
        }
    }

    fn cover(self: &Rc<Self>, monitor: &gdk::Monitor) {
        let Some(instance) = self.instance.borrow().clone() else {
            return;
        };
        let is_covered = self
            .surfaces
            .borrow()
            .iter()
            .any(|surface| surface.monitor == *monitor);
        if is_covered || !monitor.is_valid() {
            return;
        }
        let surface = self.surface(monitor);
        self.surfaces.borrow_mut().push(surface.clone());
        let was_granted = self.is_granted.get();
        instance.assign(&surface.window, monitor);
        if !monitor.is_valid() {
            self.release_surface(&surface);
            return;
        }
        if !self.is_tracked(&surface) {
            return;
        }
        let invalidate_handler = monitor.connect_local("invalidate", true, {
            let lock = Rc::downgrade(self);
            let weak_surface = Rc::downgrade(&surface);
            move |_| {
                if let (Some(lock), Some(surface)) = (lock.upgrade(), weak_surface.upgrade()) {
                    lock.release_surface(&surface);
                }
                None
            }
        });
        surface.scope.defer({
            let monitor = monitor.clone();
            move || monitor.disconnect(invalidate_handler)
        });
        surface.window.present();
        surface.field.grab_focus();
        self.watch_settled(&surface);
        if was_granted {
            self.watch_frames(&surface);
        }
    }

    fn watch_settled(self: &Rc<Self>, surface: &Rc<Surface>) {
        let lock = Rc::downgrade(self);
        let weak = Rc::downgrade(surface);
        let is_final_draw_queued = Cell::new(false);
        surface.islands.add_tick_callback(move |islands, clock| {
            let Some(surface) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if surface.is_animating(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            if !is_final_draw_queued.replace(true) {
                islands.queue_draw();
                return glib::ControlFlow::Continue;
            }
            surface.is_settled.set(true);
            if let Some(lock) = lock.upgrade() {
                lock.check_sleep();
            }
            glib::ControlFlow::Break
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_waits_until_the_lock_is_granted_and_every_surface_has_settled() {
        let cases: [(bool, &[bool], bool); 5] = [
            (false, &[true, true], false),
            (true, &[], false),
            (true, &[true, false], false),
            (true, &[true], true),
            (true, &[true, true], true),
        ];
        for (is_granted, settled_flags, is_ready) in cases {
            assert_eq!(
                is_ready_for_sleep(is_granted, settled_flags),
                is_ready,
                "{is_granted} {settled_flags:?}"
            );
        }
    }

    #[test]
    fn a_stalled_surface_is_replaced_at_most_twice() {
        let from_millis = Duration::from_millis;
        let cases = [
            (from_millis(16), 0, StallAction::Keep),
            (from_millis(999), 0, StallAction::Keep),
            (from_millis(1000), 0, StallAction::Replace),
            (from_millis(2000), 1, StallAction::Replace),
            (from_millis(2000), 2, StallAction::GiveUp),
            (from_millis(16), 2, StallAction::Keep),
        ];
        for (stall, replacement_count, stall_action) in cases {
            assert_eq!(
                choose_stall_action(stall, replacement_count),
                stall_action,
                "{stall:?} {replacement_count}"
            );
        }
    }

    #[test]
    fn the_field_does_not_clear_itself_while_the_password_is_checked() {
        let context = Rc::new(Context::default());
        context.text.replace("secret".to_owned());
        context.target.set(Some(Target::Reboot));
        context.is_in_progress.set(true);
        assert!(!context.try_expire());
        assert!(context.is_in_progress.get());
        assert_eq!(*context.text.borrow(), "secret");
        assert!(context.target() == Target::Reboot);

        context.is_in_progress.set(false);
        assert!(context.try_expire());
        assert_eq!(*context.text.borrow(), "");
        assert!(context.target() == Target::Unlock);
    }
}
