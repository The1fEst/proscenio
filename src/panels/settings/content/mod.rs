pub mod inputs;
mod layout;

use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config;
use crate::core::watch;
use crate::services::Services;
use crate::ui::theme::SharedTheme;
pub use crate::ui::widgets::input::selection::Choice;
pub use crate::ui::widgets::input::textfield::Style;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};
use crate::ui::widgets::overlay::windowdialog::WindowDialog;

pub const BASE_WIDTH: i32 = 600;
const TOP: i32 = 20;
const BOTTOM: i32 = 80;
const SECTION_SPACING: i32 = 30;
const GROUP_CLASS: &str = "settings-group";
const FOUND_CLASS: &str = "settings-found";
const FOUND_HIGHLIGHT_DURATION: std::time::Duration = std::time::Duration::from_millis(1500);
const FOUND_TOP_FRACTION: f64 = 1.0 / 3.0;
const FOUND_FRAMES: u32 = 60;

fn descendants(root: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut widgets = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        let mut child = widget.last_child();
        while let Some(current) = child {
            child = current.prev_sibling();
            stack.push(current);
        }
        widgets.push(widget);
    }
    widgets
}

fn find_label(root: &gtk4::Widget, path: &[&str], title: &str) -> Option<gtk4::Label> {
    let collect_label_texts = |widget: &gtk4::Widget| -> Vec<String> {
        descendants(widget)
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
            .map(|label| label.text().to_string())
            .collect()
    };
    let count_path_matches = |label: &gtk4::Label| {
        let mut match_count = 0;
        let mut current = label.parent();
        while let Some(widget) = current {
            if let Some(header) = widget.prev_sibling() {
                let texts = collect_label_texts(&header);
                match_count += path
                    .iter()
                    .filter(|part| texts.iter().any(|text| text == *part))
                    .count();
            }
            if &widget == root {
                break;
            }
            current = widget.parent();
        }
        match_count
    };
    descendants(root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
        .filter(|label| label.text() == title)
        .enumerate()
        .max_by_key(|(order, label)| (count_path_matches(label), std::cmp::Reverse(*order)))
        .map(|(_, label)| label)
}

fn find_reveal_target(label: &gtk4::Label) -> gtk4::Widget {
    let mut current: gtk4::Widget = label.clone().upcast();
    while let Some(parent) = current.parent() {
        if parent.has_css_class(GROUP_CLASS) {
            return current;
        }
        current = parent;
    }
    label
        .parent()
        .and_then(|centered| centered.parent())
        .unwrap_or_else(|| label.clone().upcast())
}

pub const HIGHLIGHT_CHANGED: &str = "/settings/highlightChanged";
const CHANGED_CLASS: &str = "settings-changed";

fn mark(widget: &gtk4::Widget, pointer: &str, default: &Value) {
    show_changed(widget, differs(config::value(pointer), default));
}

pub fn show_changed(widget: &impl IsA<gtk4::Widget>, is_changed: bool) {
    if is_changed && config::value_bool(HIGHLIGHT_CHANGED, false) {
        widget.add_css_class(CHANGED_CLASS);
    } else {
        widget.remove_css_class(CHANGED_CLASS);
    }
}

pub trait Parent {
    fn add(&self, child: &impl IsA<gtk4::Widget>);
}

impl Parent for gtk4::Box {
    fn add(&self, child: &impl IsA<gtk4::Widget>) {
        self.append(child);
    }
}

impl Parent for Row {
    fn add(&self, child: &impl IsA<gtk4::Widget>) {
        self.append(child);
    }
}

type Subpage = Rc<dyn Fn(&'static str, &'static str)>;

struct Repeating(Option<glib::SourceId>);

impl Drop for Repeating {
    fn drop(&mut self) {
        if let Some(source) = self.0.take() {
            source.remove();
        }
    }
}

pub struct Context {
    pub theme: SharedTheme,
    pub services: Rc<Services>,
    pub on_open_subpage: RefCell<Option<Subpage>>,
    pub overlay: glib::WeakRef<gtk4::Overlay>,
    pub dialog: Rc<RefCell<Option<Rc<WindowDialog>>>>,
    pub argument: Rc<RefCell<Option<String>>>,
    pub heading: glib::WeakRef<gtk4::Label>,
    pub on_back: RefCell<Option<Rc<dyn Fn()>>>,
    pub shared: RefCell<Option<(&'static str, Rc<dyn Any>)>>,
}

impl Context {
    pub fn dialog_presenter(&self) -> impl Fn(Rc<WindowDialog>) + use<> {
        let overlay = self.overlay.clone();
        let current_dialog = self.dialog.clone();
        move |dialog| {
            let Some(overlay) = overlay.upgrade() else {
                return;
            };
            if let Some(previous) = current_dialog.take() {
                previous.remove_from(&overlay);
            }
            dialog.set_scrim_radius(0.0);
            dialog.connect_dismiss({
                let dialog = Rc::downgrade(&dialog);
                let overlay = overlay.downgrade();
                let current_dialog = current_dialog.clone();
                move || {
                    let (Some(dialog), Some(overlay)) = (dialog.upgrade(), overlay.upgrade())
                    else {
                        return;
                    };
                    let root = dialog.root.downgrade();
                    let overlay = overlay.downgrade();
                    let current_dialog = current_dialog.clone();
                    dialog.show(false, move || {
                        if let (Some(overlay), Some(root)) = (overlay.upgrade(), root.upgrade()) {
                            overlay.remove_overlay(&root);
                        }
                        current_dialog.take();
                    });
                }
            });
            overlay.add_overlay(&dialog.root);
            dialog.show(true, || {});
            current_dialog.replace(Some(dialog));
        }
    }

    pub fn subpage_opener(&self, id: &'static str) -> impl Fn() + use<> {
        let open = self.on_open_subpage.borrow().clone();
        let title = super::pages::subpage(id).map_or(id, |subpage| subpage.title);
        let argument = self.argument.clone();
        move || {
            argument.take();
            if let Some(open) = open.as_ref() {
                open(title, id);
            }
        }
    }

    pub fn subpage_opener_with(&self, id: &'static str) -> impl Fn(&str) + use<> {
        let open = self.on_open_subpage.borrow().clone();
        let title = super::pages::subpage(id).map_or(id, |subpage| subpage.title);
        let argument = self.argument.clone();
        move |value| {
            argument.replace(Some(value.to_owned()));
            if let Some(open) = open.as_ref() {
                open(title, id);
            }
        }
    }

    pub fn go_back(&self) -> impl Fn() + use<> {
        let on_back = self.on_back.borrow().clone();
        move || {
            if let Some(on_back) = on_back.as_ref() {
                on_back();
            }
        }
    }
}

struct Tracked {
    pointer: String,
    default: Value,
    widget: glib::WeakRef<gtk4::Widget>,
}

pub fn differs(value: Option<Value>, default: &Value) -> bool {
    let Some(value) = value else {
        return false;
    };
    match (value.as_f64(), default.as_f64()) {
        (Some(number), Some(default_number)) => (number - default_number).abs() > f64::EPSILON,
        _ => value != *default,
    }
}

pub struct Page {
    pub root: gtk4::ScrolledWindow,
    column: gtk4::Box,
    pub theme: SharedTheme,
    owned_items: RefCell<Vec<Box<dyn Any>>>,
    tracked_settings: RefCell<Vec<Tracked>>,
}

impl Page {
    pub fn track(&self, pointer: &str, default: Value, widget: &impl IsA<gtk4::Widget>) {
        let widget: gtk4::Widget = widget.clone().upcast();
        mark(&widget, pointer, &default);
        self.tracked_settings.borrow_mut().push(Tracked {
            pointer: pointer.to_owned(),
            default: default.clone(),
            widget: widget.downgrade(),
        });
        let pointer = pointer.to_owned();
        let widget = widget.downgrade();
        self.watch(&pointer.clone(), move || {
            if let Some(widget) = widget.upgrade() {
                mark(&widget, &pointer, &default);
            }
        });
    }

    pub fn changed_settings(&self) -> usize {
        self.tracked_settings
            .borrow()
            .iter()
            .filter(|setting| differs(config::value(&setting.pointer), &setting.default))
            .count()
    }

    pub fn reset_to_defaults(&self) {
        let changes: Vec<(String, Value)> = self
            .tracked_settings
            .borrow()
            .iter()
            .filter(|setting| differs(config::value(&setting.pointer), &setting.default))
            .map(|setting| (setting.pointer.clone(), setting.default.clone()))
            .collect();
        for (pointer, default) in changes {
            config::store_value(&pointer, default);
        }
    }

    pub fn refresh_marks(&self) {
        for setting in self.tracked_settings.borrow().iter() {
            if let Some(widget) = setting.widget.upgrade() {
                mark(&widget, &setting.pointer, &setting.default);
            }
        }
    }

    pub fn new(theme: &SharedTheme, should_force_width: bool) -> Rc<Self> {
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING);
        column.set_margin_top(TOP);
        column.set_margin_bottom(BOTTOM);
        column.set_halign(gtk4::Align::Center);
        column.set_size_request(BASE_WIDTH, -1);
        let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        if should_force_width {
            let frame = crate::ui::widgets::layout::fixedwidth::FixedWidth::new(BASE_WIDTH);
            frame.set_child(&column);
            frame.set_halign(gtk4::Align::Center);
            holder.append(&frame);
        } else {
            holder.append(&column);
        }
        let root = gtk4::ScrolledWindow::new();
        root.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        root.set_vexpand(true);
        root.set_child(Some(&holder));
        crate::ui::widgets::input::flickable::follow_scroll_settings(&root);
        let page = Rc::new(Page {
            root,
            column,
            theme: theme.clone(),
            owned_items: RefCell::new(Vec::new()),
            tracked_settings: RefCell::new(Vec::new()),
        });
        page.watch(HIGHLIGHT_CHANGED, {
            let page = Rc::downgrade(&page);
            move || {
                if let Some(page) = page.upgrade() {
                    page.refresh_marks();
                }
            }
        });
        page
    }

    pub fn keep(&self, item: impl Any) {
        self.owned_items.borrow_mut().push(Box::new(item));
    }

    pub fn reveal(&self, path: &[&str], title: &str) {
        let column = self.column.downgrade();
        let path: Vec<String> = path.iter().map(|part| (*part).to_owned()).collect();
        let title = title.to_owned();
        let target_slot: RefCell<Option<gtk4::Widget>> = RefCell::new(None);
        let frame_count = Cell::new(0);
        self.root.add_tick_callback(move |scroller, _| {
            frame_count.set(frame_count.get() + 1);
            let wait_or_give_up = || {
                if frame_count.get() < FOUND_FRAMES {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            };
            if target_slot.borrow().is_none() {
                let Some(column) = column.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let parts: Vec<&str> = path.iter().map(String::as_str).collect();
                let label = find_label(column.upcast_ref(), &parts, &title);
                target_slot.replace(label.map(|label| find_reveal_target(&label)));
            }
            let Some(target) = target_slot.borrow().clone() else {
                return wait_or_give_up();
            };
            let content = scroller.child().and_then(|viewport| viewport.first_child());
            let bounds = content.and_then(|content| target.compute_bounds(&content));
            let adjustment = scroller.vadjustment();
            let Some(bounds) =
                bounds.filter(|bounds| bounds.height() > 0.0 && adjustment.page_size() > 0.0)
            else {
                return wait_or_give_up();
            };
            adjustment.set_value(bounds.y() as f64 - adjustment.page_size() * FOUND_TOP_FRACTION);
            target.add_css_class(FOUND_CLASS);
            let target = target.downgrade();
            glib::timeout_add_local_once(FOUND_HIGHLIGHT_DURATION, move || {
                if let Some(target) = target.upgrade() {
                    target.remove_css_class(FOUND_CLASS);
                }
            });
            glib::ControlFlow::Break
        });
    }

    pub fn watch(&self, pointer: &str, action: impl Fn() + 'static) {
        self.keep(watch::config(pointer, action));
    }

    pub fn every(&self, interval: std::time::Duration, action: impl Fn() + 'static) {
        action();
        let source = glib::timeout_add_local(interval, move || {
            action();
            glib::ControlFlow::Continue
        });
        self.keep(Repeating(Some(source)));
    }

    pub fn tip(&self, target: &impl IsA<gtk4::Widget>, text: &str) {
        self.keep(self.unkept_tip(target, text));
    }

    pub fn unkept_tip(&self, target: &impl IsA<gtk4::Widget>, text: &str) -> Rc<Tooltip> {
        let tip = Tooltip::new(target, &self.theme, tooltip::Kind::Styled);
        tip.place_like_qt();
        tip.set_text(text);
        tooltip::hover_delay(target, &tip, 0);
        tip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_differs_only_when_stored_and_not_equal_to_its_default() {
        assert!(!differs(None, &Value::from(5)));
        assert!(!differs(Some(Value::from(5.0)), &Value::from(5)));
        assert!(differs(Some(Value::from(6)), &Value::from(5)));
        assert!(!differs(Some(Value::from("dd/MM")), &Value::from("dd/MM")));
        assert!(differs(Some(Value::Bool(true)), &Value::Bool(false)));
        assert!(differs(
            Some(Value::from(vec!["a"])),
            &Value::from(Vec::<String>::new())
        ));
    }
}
