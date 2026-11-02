mod confirmation;
mod editor;
mod eventdialog;
mod findbar;
mod matches;
mod privacy;
mod symbolbutton;
mod tabs;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::scope::Scope;
use crate::platform::wayland::grab;
use crate::services::Services;
use crate::services::session::agenda::{self, Agenda};
use crate::services::session::notes::Notes;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::overlay::tooltip;
use crate::ui::widgets::overlay::windowdialog::WindowDialog;
use editor::Editor;
use findbar::FindBar;
use privacy::Privacy;
use tabs::Tabs;

const NAMESPACE: &str = "proscenio:sidebarLeft";
const WIDTH: i32 = 480;
const GAP: i32 = 5;
const ELEVATION: i32 = 10;
const PADDING: i32 = 10;
const FIND_BUTTON_SIZE: i32 = 32;

pub struct LeftSidebar {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    notes: Rc<Notes>,
    agenda: Rc<Agenda>,
    editor: Rc<Editor>,
    tabs: Rc<Tabs>,
    find: Rc<FindBar>,
    overlay: gtk4::Overlay,
    theme: SharedTheme,
    dialog: RefCell<Option<Rc<WindowDialog>>>,
    last_active_id: Cell<u64>,
    privacy: Privacy,
    watchers: Listeners<bool>,
}

impl LeftSidebar {
    pub fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn open(self: &Rc<Self>) {
        if !self.window.is_visible() {
            self.show();
        }
    }

    pub fn open_note(self: &Rc<Self>, id: u64) -> bool {
        if self.notes.status(id).is_none() {
            return false;
        }
        self.notes.select(id);
        self.open();
        true
    }

    pub fn close(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide();
        }
    }

    pub fn watch(&self, watcher: impl Fn(bool) + 'static) -> Subscription {
        watcher(self.window.is_visible());
        self.watchers.add_with(move |is_open| watcher(*is_open))
    }

    fn announce(&self) {
        self.watchers.notify_with(&self.window.is_visible());
    }

    fn show(self: &Rc<Self>) {
        self.window.set_visible(true);
        self.announce();
        self.privacy.show();
        self.editor.focus();
        let (Some(grab), Some(surface)) = (self.grab.as_ref(), self.window.surface()) else {
            return;
        };
        let left_sidebar = Rc::downgrade(self);
        grab.hold(&surface, move || {
            if let Some(left_sidebar) = left_sidebar.upgrade() {
                left_sidebar.hide();
            }
        });
    }

    fn hide(self: &Rc<Self>) {
        if let Some(grab) = self.grab.as_ref() {
            grab.release();
        }
        if let Some(dialog) = self.dialog.take() {
            dialog.remove_from(&self.overlay);
        }
        self.tabs.cancel_rename();
        self.find.hide();
        self.notes.flush();
        self.window.set_visible(false);
        self.announce();
    }

    fn open_find(self: &Rc<Self>) {
        self.find.open(self.editor.selected_line().as_deref());
    }

    fn refresh(self: &Rc<Self>) {
        self.tabs.refresh();
        self.editor.refresh();
        self.find.set_buffer(self.editor.text_buffer());
        let active = self.notes.active_id();
        if self.last_active_id.replace(active) != active && self.window.is_visible() {
            self.editor.focus();
        }
    }

    fn step_tab(&self, delta: i32) {
        let summaries = self.notes.summaries();
        let active = self.notes.active_id();
        let Some(position) = summaries.iter().position(|summary| summary.id == active) else {
            return;
        };
        let target = (position as i32 + delta).clamp(0, summaries.len() as i32 - 1);
        self.notes.select(summaries[target as usize].id);
    }

    fn request_close(self: &Rc<Self>, id: u64) {
        if self.notes.is_empty(id) {
            self.notes.remove(id);
            return;
        }
        let dialog = confirmation::build(&self.theme, {
            let notes = self.notes.clone();
            move || notes.remove(id)
        });
        self.present(dialog);
    }

    fn request_event(self: &Rc<Self>) {
        let id = self.notes.active_id();
        let Some(buffer) = self.notes.buffer(id) else {
            return;
        };
        let title = self
            .notes
            .summaries()
            .into_iter()
            .find(|summary| summary.id == id)
            .and_then(|summary| summary.title().map(str::to_owned))
            .unwrap_or_else(|| tr("Untitled"));
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        let dialog = eventdialog::build(
            &self.theme,
            &self.agenda,
            eventdialog::Note {
                id,
                title,
                excerpt: agenda::make_excerpt(&text),
            },
            &glib::DateTime::now_local().expect("local time"),
        );
        self.present(dialog);
    }

    fn present(self: &Rc<Self>, dialog: Rc<WindowDialog>) {
        if let Some(previous) = self.dialog.take() {
            previous.remove_from(&self.overlay);
        }
        dialog.root.set_margin_top(GAP);
        dialog.root.set_margin_bottom(GAP);
        dialog.root.set_margin_start(GAP);
        dialog.root.set_margin_end(ELEVATION);
        dialog.connect_dismiss({
            let dialog = Rc::downgrade(&dialog);
            let left_sidebar = Rc::downgrade(self);
            move || {
                let (Some(dialog), Some(left_sidebar)) = (dialog.upgrade(), left_sidebar.upgrade())
                else {
                    return;
                };
                let root = dialog.root.downgrade();
                let left_sidebar = Rc::downgrade(&left_sidebar);
                dialog.show(false, move || {
                    let Some(left_sidebar) = left_sidebar.upgrade() else {
                        return;
                    };
                    if let Some(root) = root.upgrade() {
                        left_sidebar.overlay.remove_overlay(&root);
                    }
                    left_sidebar.dialog.take();
                    left_sidebar.editor.focus();
                });
            }
        });
        self.overlay.add_overlay(&dialog.root);
        dialog.show(true, || {});
        self.dialog.replace(Some(dialog));
    }
}

pub fn build(
    app: &gtk4::Application,
    theme: &SharedTheme,
    services: &Rc<Services>,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Rc<LeftSidebar> {
    let notes = services.notes.clone();
    let owner: Rc<RefCell<Weak<LeftSidebar>>> = Rc::new(RefCell::new(Weak::new()));

    let editor = Editor::new(&notes);
    let tabs = Tabs::new(
        theme,
        &notes,
        Rc::new({
            let owner = owner.clone();
            move |id| {
                if let Some(left_sidebar) = owner.borrow().upgrade() {
                    left_sidebar.request_close(id);
                }
            }
        }),
        Rc::new({
            let editor = Rc::downgrade(&editor);
            move || {
                if let Some(editor) = editor.upgrade() {
                    editor.focus();
                }
            }
        }),
    );

    let find = FindBar::new(
        theme,
        Rc::new({
            let editor = Rc::downgrade(&editor);
            move |range| {
                if let Some(editor) = editor.upgrade() {
                    editor.select(range);
                }
            }
        }),
        Rc::new({
            let editor = Rc::downgrade(&editor);
            move || {
                if let Some(editor) = editor.upgrade() {
                    editor.focus();
                }
            }
        }),
    );

    let find_button = symbolbutton::build(theme, "search", pixel_size::HUGE, FIND_BUTTON_SIZE);
    find_button.connect_clicked({
        let owner = owner.clone();
        move |_| {
            if let Some(left_sidebar) = owner.borrow().upgrade() {
                left_sidebar.open_find();
            }
        }
    });

    let event_button = symbolbutton::build(theme, "event", pixel_size::HUGE, FIND_BUTTON_SIZE);
    event_button.connect_clicked({
        let owner = owner.clone();
        move |_| {
            if let Some(left_sidebar) = owner.borrow().upgrade() {
                left_sidebar.request_event();
            }
        }
    });
    tooltip::attach_hover_tip(&event_button, theme, &tr("Add to calendar"));

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    header.append(&tabs.widget);
    header.append(&event_button);
    header.append(&find_button);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.add_css_class("notepad");
    column.set_margin_top(GAP);
    column.set_margin_bottom(GAP);
    column.set_margin_start(GAP);
    column.set_margin_end(ELEVATION);
    header.set_margin_bottom(PADDING);
    tabs.widget.set_hexpand(true);
    column.append(&header);
    column.append(&find.widget);
    column.append(&editor.widget);
    let privacy = Privacy::new(&services.device_usage);
    column.append(&privacy.widget);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&column));

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .default_width(WIDTH)
        .child(&overlay)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_visible(false);

    let left_sidebar = Rc::new(LeftSidebar {
        window: window.clone(),
        grab: grab::Grab::new(&monitor.display()),
        notes: notes.clone(),
        agenda: services.agenda.clone(),
        editor,
        tabs,
        find,
        overlay,
        theme: theme.clone(),
        dialog: RefCell::new(None),
        last_active_id: Cell::new(notes.active_id()),
        privacy,
        watchers: Listeners::default(),
    });
    owner.replace(Rc::downgrade(&left_sidebar));
    left_sidebar.refresh();
    scope.keep(services.device_usage.subscribe({
        let left_sidebar = Rc::downgrade(&left_sidebar);
        move || {
            if let Some(left_sidebar) = left_sidebar.upgrade()
                && left_sidebar.window.is_visible()
            {
                left_sidebar.privacy.refresh();
            }
        }
    }));
    scope.keep(notes.subscribe({
        let left_sidebar = Rc::downgrade(&left_sidebar);
        move || {
            if let Some(left_sidebar) = left_sidebar.upgrade() {
                left_sidebar.refresh();
            }
        }
    }));

    scope.keep(notes.subscribe_rejection({
        let left_sidebar = Rc::downgrade(&left_sidebar);
        move || {
            if let Some(left_sidebar) = left_sidebar.upgrade() {
                left_sidebar.editor.warn_full();
            }
        }
    }));

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    key_controller.connect_key_pressed({
        let left_sidebar = Rc::downgrade(&left_sidebar);
        move |_, key, _, modifiers| {
            let Some(left_sidebar) = left_sidebar.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if left_sidebar.dialog.borrow().is_some() {
                return glib::Propagation::Proceed;
            }
            let is_control_held = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            match key {
                gdk::Key::Escape if left_sidebar.tabs.is_renaming() => {
                    left_sidebar.tabs.cancel_rename()
                }
                gdk::Key::Escape if left_sidebar.find.is_open() => left_sidebar.find.close(),
                gdk::Key::Escape => left_sidebar.hide(),
                gdk::Key::f | gdk::Key::F if is_control_held => left_sidebar.open_find(),
                gdk::Key::Page_Down if is_control_held => left_sidebar.step_tab(1),
                gdk::Key::Page_Up if is_control_held => left_sidebar.step_tab(-1),
                gdk::Key::n | gdk::Key::N if is_control_held => left_sidebar.notes.create(),
                gdk::Key::w | gdk::Key::W if is_control_held => {
                    left_sidebar.request_close(left_sidebar.notes.active_id())
                }
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        }
    });
    window.add_controller(key_controller);

    left_sidebar
}
