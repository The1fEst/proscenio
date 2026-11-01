mod confirmation;
mod editor;
mod tabs;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::scope::Scope;
use crate::platform::wayland::grab;
use crate::services::Services;
use crate::services::session::notes::Notes;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::overlay::windowdialog::WindowDialog;
use editor::Editor;
use tabs::Tabs;

const NAMESPACE: &str = "proscenio:notepad";
const WIDTH: i32 = 480;
const GAP: i32 = 5;
const ELEVATION: i32 = 10;
const PADDING: i32 = 10;

pub struct Notepad {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    notes: Rc<Notes>,
    editor: Rc<Editor>,
    tabs: Rc<Tabs>,
    overlay: gtk4::Overlay,
    theme: SharedTheme,
    dialog: RefCell<Option<Rc<WindowDialog>>>,
    last_active_id: Cell<u64>,
}

impl Notepad {
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

    pub fn close(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide();
        }
    }

    fn show(self: &Rc<Self>) {
        self.window.set_visible(true);
        self.editor.focus();
        let (Some(grab), Some(surface)) = (self.grab.as_ref(), self.window.surface()) else {
            return;
        };
        let notepad = Rc::downgrade(self);
        grab.hold(&surface, move || {
            if let Some(notepad) = notepad.upgrade() {
                notepad.hide();
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
        self.notes.flush();
        self.window.set_visible(false);
    }

    fn refresh(&self) {
        self.tabs.refresh();
        self.editor.refresh();
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
        if let Some(previous) = self.dialog.take() {
            previous.remove_from(&self.overlay);
        }
        let dialog = confirmation::build(&self.theme, {
            let notes = self.notes.clone();
            move || notes.remove(id)
        });
        dialog.root.set_margin_top(GAP);
        dialog.root.set_margin_bottom(GAP);
        dialog.root.set_margin_start(GAP);
        dialog.root.set_margin_end(ELEVATION);
        dialog.connect_dismiss({
            let dialog = Rc::downgrade(&dialog);
            let notepad = Rc::downgrade(self);
            move || {
                let (Some(dialog), Some(notepad)) = (dialog.upgrade(), notepad.upgrade()) else {
                    return;
                };
                let root = dialog.root.downgrade();
                let notepad = Rc::downgrade(&notepad);
                dialog.show(false, move || {
                    let Some(notepad) = notepad.upgrade() else {
                        return;
                    };
                    if let Some(root) = root.upgrade() {
                        notepad.overlay.remove_overlay(&root);
                    }
                    notepad.dialog.take();
                    notepad.editor.focus();
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
) -> Rc<Notepad> {
    let notes = services.notes.clone();
    let owner: Rc<RefCell<Weak<Notepad>>> = Rc::new(RefCell::new(Weak::new()));

    let editor = Editor::new(&notes);
    let tabs = Tabs::new(
        theme,
        &notes,
        Rc::new({
            let owner = owner.clone();
            move |id| {
                if let Some(notepad) = owner.borrow().upgrade() {
                    notepad.request_close(id);
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

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, PADDING);
    column.add_css_class("notepad");
    column.set_margin_top(GAP);
    column.set_margin_bottom(GAP);
    column.set_margin_start(GAP);
    column.set_margin_end(ELEVATION);
    column.append(&tabs.widget);
    column.append(&editor.widget);

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

    let notepad = Rc::new(Notepad {
        window: window.clone(),
        grab: grab::Grab::new(&monitor.display()),
        notes: notes.clone(),
        editor,
        tabs,
        overlay,
        theme: theme.clone(),
        dialog: RefCell::new(None),
        last_active_id: Cell::new(notes.active_id()),
    });
    owner.replace(Rc::downgrade(&notepad));
    notepad.refresh();
    scope.keep(notes.subscribe({
        let notepad = Rc::downgrade(&notepad);
        move || {
            if let Some(notepad) = notepad.upgrade() {
                notepad.refresh();
            }
        }
    }));

    scope.keep(notes.subscribe_rejection({
        let notepad = Rc::downgrade(&notepad);
        move || {
            if let Some(notepad) = notepad.upgrade() {
                notepad.editor.warn_full();
            }
        }
    }));

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    key_controller.connect_key_pressed({
        let notepad = Rc::downgrade(&notepad);
        move |_, key, _, modifiers| {
            let Some(notepad) = notepad.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if notepad.dialog.borrow().is_some() {
                return glib::Propagation::Proceed;
            }
            let is_control_held = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            match key {
                gdk::Key::Escape if notepad.tabs.is_renaming() => notepad.tabs.cancel_rename(),
                gdk::Key::Escape => notepad.hide(),
                gdk::Key::Page_Down if is_control_held => notepad.step_tab(1),
                gdk::Key::Page_Up if is_control_held => notepad.step_tab(-1),
                gdk::Key::n | gdk::Key::N if is_control_held => notepad.notes.create(),
                gdk::Key::w | gdk::Key::W if is_control_held => {
                    notepad.request_close(notepad.notes.active_id())
                }
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        }
    });
    window.add_controller(key_controller);

    notepad
}
