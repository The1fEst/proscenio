use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config;
use crate::core::tools::{self, Tool};
use crate::core::watch;
use crate::services::Services;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::{self, ComboBox, ConfigSwitch};
use crate::ui::widgets::loading::LoadingIndicator;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::row::Row;
pub use crate::ui::widgets::selection::Choice;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::slider::{Options, Slider};
use crate::ui::widgets::spinbox::SpinBox;
use crate::ui::widgets::text;
pub use crate::ui::widgets::textfield::Style;
use crate::ui::widgets::textfield::TextField;
use crate::ui::widgets::tooltip::{self, Tooltip};
use crate::ui::widgets::windowdialog::WindowDialog;

pub const BASE_WIDTH: i32 = 600;
const TOP: i32 = 20;
const BOTTOM: i32 = 80;
const SECTION_SPACING: i32 = 30;
const SECTION_HEADER_SPACING: i32 = 6;
const SECTION_CONTENT_SPACING: i32 = 4;
const SUBSECTION_TOP: i32 = 4;
const SUBSECTION_SPACING: i32 = 2;
const SUBSECTION_LABEL_START: i32 = 2;
const ROW_MARGIN: i32 = 8;
const ROW_SPACING: i32 = 10;
const ROW_GAP: i32 = 4;
const LINK_HEIGHT: i32 = 56;
const LINK_PADDING: i32 = 12;
const LINK_SPACING: i32 = 12;
const NOTICE_SPACING: i32 = 8;
const BUSY_SIZE: i32 = 18;
const BUSY_MARGIN: i32 = 4;
const DISABLED_OPACITY: f64 = 0.4;
const SLIDER_TRACK: f64 = 12.0;
const SLIDER_LABEL: i32 = 120;
const SLIDER_SPACING: i32 = 10;
const SLIDER_MARGIN: i32 = 8;
const GROUP_CLASS: &str = "settings-group";
const FOUND_CLASS: &str = "settings-found";
const FOUND_SHOWN: std::time::Duration = std::time::Duration::from_millis(1500);
const FOUND_ABOVE: f64 = 1.0 / 3.0;
const FOUND_FRAMES: u32 = 60;

/// The QML symbol loader gives its item a width and no height, so a row sizes to its
/// text and the symbol hangs across the middle of it.
fn without_height(child: &impl IsA<gtk4::Widget>) -> gtk4::Widget {
    let width = child.measure(gtk4::Orientation::Horizontal, -1).1;
    let slot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    slot.set_size_request(width, 0);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&slot));
    child.set_valign(gtk4::Align::Center);
    overlay.add_overlay(child);
    overlay.set_valign(gtk4::Align::Center);
    overlay.upcast()
}

fn descendants(root: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        let mut child = widget.last_child();
        while let Some(current) = child {
            child = current.prev_sibling();
            stack.push(current);
        }
        found.push(widget);
    }
    found
}

fn find_label(root: &gtk4::Widget, path: &[&str], title: &str) -> Option<gtk4::Label> {
    let label_texts = |widget: &gtk4::Widget| -> Vec<String> {
        descendants(widget)
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
            .map(|label| label.text().to_string())
            .collect()
    };
    let headed_by = |label: &gtk4::Label| {
        let mut matched = 0;
        let mut current = label.parent();
        while let Some(widget) = current {
            if let Some(header) = widget.prev_sibling() {
                let texts = label_texts(&header);
                matched += path
                    .iter()
                    .filter(|part| texts.iter().any(|text| text == *part))
                    .count();
            }
            if &widget == root {
                break;
            }
            current = widget.parent();
        }
        matched
    };
    descendants(root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
        .filter(|label| label.text() == title)
        .enumerate()
        .max_by_key(|(order, label)| (headed_by(label), std::cmp::Reverse(*order)))
        .map(|(_, label)| label)
}

fn found_target(label: &gtk4::Label) -> gtk4::Widget {
    let mut current: gtk4::Widget = label.clone().upcast();
    while let Some(parent) = current.parent() {
        if parent.has_css_class(GROUP_CLASS) {
            return current;
        }
        current = parent;
    }
    label
        .parent()
        .and_then(|centred| centred.parent())
        .unwrap_or_else(|| label.clone().upcast())
}

pub const HIGHLIGHT_CHANGED: &str = "/settings/highlightChanged";
const CHANGED_CLASS: &str = "settings-changed";

fn mark(widget: &gtk4::Widget, pointer: &str, default: &Value) {
    show_changed(widget, differs(config::value(pointer), default));
}

pub fn show_changed(widget: &impl IsA<gtk4::Widget>, changed: bool) {
    if changed && config::value_bool(HIGHLIGHT_CHANGED, false) {
        widget.add_css_class(CHANGED_CLASS);
    } else {
        widget.remove_css_class(CHANGED_CLASS);
    }
}

pub trait Parent {
    fn add(&self, child: &impl IsA<gtk4::Widget>);
}

pub fn slider_row(
    theme: &SharedTheme,
    parent: &impl Parent,
    icon: &str,
    label: &str,
    range: (f64, f64),
) -> (Rc<Slider>, gtk4::Label) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SLIDER_SPACING);
    row.set_margin_start(SLIDER_MARGIN);
    row.set_margin_end(SLIDER_MARGIN);
    let symbol = text::symbol(icon, pixel_size::LARGER as f64);
    text::set_color(&symbol, "m3onBackground");
    row.append(&Centred::integral(&symbol));
    let name = text::styled(label);
    text::set_color(&name, "colOnSecondaryContainer");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    name.set_max_width_chars(1);
    name.set_size_request(SLIDER_LABEL, -1);
    row.append(&Centred::new(&name));
    let slider = Slider::with(
        theme,
        Options {
            track: SLIDER_TRACK,
            from: range.0,
            to: range.1,
            icon: None,
            secondary: None,
            dividers: Vec::new(),
        },
    );
    slider.set_stops(vec![1.0]);
    slider.area.set_hexpand(true);
    row.append(&slider.area);
    parent.add(&row);
    (slider, symbol)
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
    pub subpage: RefCell<Option<Subpage>>,
    pub overlay: glib::WeakRef<gtk4::Overlay>,
    pub dialog: Rc<RefCell<Option<Rc<WindowDialog>>>>,
    pub argument: Rc<RefCell<Option<String>>>,
    pub heading: glib::WeakRef<gtk4::Label>,
    pub back: RefCell<Option<Rc<dyn Fn()>>>,
    pub shared: RefCell<Option<(&'static str, Rc<dyn Any>)>>,
}

impl Context {
    pub fn dialog_presenter(&self) -> impl Fn(Rc<WindowDialog>) + use<> {
        let overlay = self.overlay.clone();
        let shown = self.dialog.clone();
        move |dialog| {
            let Some(overlay) = overlay.upgrade() else {
                return;
            };
            if let Some(previous) = shown.take() {
                overlay.remove_overlay(&previous.root);
            }
            dialog.set_scrim_radius(0.0);
            dialog.connect_dismiss({
                let dialog = Rc::downgrade(&dialog);
                let overlay = overlay.downgrade();
                let shown = shown.clone();
                move || {
                    let (Some(dialog), Some(overlay)) = (dialog.upgrade(), overlay.upgrade())
                    else {
                        return;
                    };
                    let root = dialog.root.clone();
                    let shown = shown.clone();
                    dialog.show(false, move || {
                        overlay.remove_overlay(&root);
                        shown.take();
                    });
                }
            });
            overlay.add_overlay(&dialog.root);
            dialog.show(true, || {});
            shown.replace(Some(dialog));
        }
    }

    pub fn subpage_opener(&self, id: &'static str) -> impl Fn() + use<> {
        let open = self.subpage.borrow().clone();
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
        let open = self.subpage.borrow().clone();
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
        let back = self.back.borrow().clone();
        move || {
            if let Some(back) = back.as_ref() {
                back();
            }
        }
    }
}

struct Tracked {
    pointer: String,
    default: Value,
    widget: glib::WeakRef<gtk4::Widget>,
}

pub fn differs(stored: Option<Value>, default: &Value) -> bool {
    let Some(stored) = stored else {
        return false;
    };
    match (stored.as_f64(), default.as_f64()) {
        (Some(stored), Some(default)) => (stored - default).abs() > f64::EPSILON,
        _ => stored != *default,
    }
}

pub struct Page {
    pub root: gtk4::ScrolledWindow,
    column: gtk4::Box,
    pub theme: SharedTheme,
    kept: RefCell<Vec<Box<dyn Any>>>,
    tracked: RefCell<Vec<Tracked>>,
}

impl Page {
    pub fn track(&self, pointer: &str, default: Value, widget: &impl IsA<gtk4::Widget>) {
        let widget: gtk4::Widget = widget.clone().upcast();
        mark(&widget, pointer, &default);
        self.tracked.borrow_mut().push(Tracked {
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
        self.tracked
            .borrow()
            .iter()
            .filter(|tracked| differs(config::value(&tracked.pointer), &tracked.default))
            .count()
    }

    pub fn reset_to_defaults(&self) {
        let changed: Vec<(String, Value)> = self
            .tracked
            .borrow()
            .iter()
            .filter(|tracked| differs(config::value(&tracked.pointer), &tracked.default))
            .map(|tracked| (tracked.pointer.clone(), tracked.default.clone()))
            .collect();
        for (pointer, default) in changed {
            config::store_value(&pointer, default);
        }
    }

    pub fn refresh_marks(&self) {
        for tracked in self.tracked.borrow().iter() {
            if let Some(widget) = tracked.widget.upgrade() {
                mark(&widget, &tracked.pointer, &tracked.default);
            }
        }
    }

    pub fn new(theme: &SharedTheme, force_width: bool) -> Rc<Self> {
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING);
        column.set_margin_top(TOP);
        column.set_margin_bottom(BOTTOM);
        column.set_halign(gtk4::Align::Center);
        column.set_size_request(BASE_WIDTH, -1);
        let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        if force_width {
            let frame = crate::ui::widgets::fixedwidth::FixedWidth::new(BASE_WIDTH);
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
        crate::ui::widgets::flickable::follow_scroll_settings(&root);
        let page = Rc::new(Page {
            root,
            column,
            theme: theme.clone(),
            kept: RefCell::new(Vec::new()),
            tracked: RefCell::new(Vec::new()),
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

    pub fn keep(&self, held: impl Any) {
        self.kept.borrow_mut().push(Box::new(held));
    }

    pub fn reveal(&self, path: &[&str], title: &str) {
        let column = self.column.downgrade();
        let path: Vec<String> = path.iter().map(|part| (*part).to_owned()).collect();
        let title = title.to_owned();
        let found: RefCell<Option<gtk4::Widget>> = RefCell::new(None);
        let frames = Cell::new(0);
        self.root.add_tick_callback(move |scroller, _| {
            frames.set(frames.get() + 1);
            let waiting = || {
                if frames.get() < FOUND_FRAMES {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            };
            if found.borrow().is_none() {
                let Some(column) = column.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let parts: Vec<&str> = path.iter().map(String::as_str).collect();
                let label = find_label(column.upcast_ref(), &parts, &title);
                found.replace(label.map(|label| found_target(&label)));
            }
            let Some(target) = found.borrow().clone() else {
                return waiting();
            };
            let content = scroller.child().and_then(|viewport| viewport.first_child());
            let bounds = content.and_then(|content| target.compute_bounds(&content));
            let adjustment = scroller.vadjustment();
            let Some(bounds) =
                bounds.filter(|bounds| bounds.height() > 0.0 && adjustment.page_size() > 0.0)
            else {
                return waiting();
            };
            adjustment.set_value(bounds.y() as f64 - adjustment.page_size() * FOUND_ABOVE);
            target.add_css_class(FOUND_CLASS);
            let target = target.downgrade();
            glib::timeout_add_local_once(FOUND_SHOWN, move || {
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

    pub fn section(&self, icon: &str, title: &str) -> gtk4::Box {
        Self::section_into(&self.column, icon, title)
    }

    pub fn sections() -> gtk4::Box {
        gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING)
    }

    pub fn section_into(column: &gtk4::Box, icon: &str, title: &str) -> gtk4::Box {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_HEADER_SPACING);
        if !icon.is_empty() || !title.is_empty() {
            let header = gtk4::Box::new(gtk4::Orientation::Horizontal, SECTION_HEADER_SPACING);
            if !icon.is_empty() {
                let symbol = text::symbol(icon, pixel_size::HUGEASS as f64);
                text::set_color(&symbol, "colOnSecondaryContainer");
                header.append(&without_height(&Centred::integral(&symbol)));
            }
            let name = text::styled(title);
            text::set_font(
                &name,
                text::Family::Main,
                pixel_size::LARGER as f64,
                "wght=450",
            );
            text::set_color(&name, "colOnSecondaryContainer");
            header.append(&Centred::new(&name));
            section.append(&header);
        }
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_CONTENT_SPACING);
        content.add_css_class(GROUP_CLASS);
        section.append(&content);
        column.append(&section);
        content
    }

    pub fn busy_section(&self, icon: &str, title: &str) -> (gtk4::Box, Rc<LoadingIndicator>) {
        let content = self.section(icon, title);
        let indicator = LoadingIndicator::new(&self.theme, BUSY_SIZE);
        indicator.area.set_margin_start(BUSY_MARGIN);
        indicator.area.set_visible(false);
        if let Some(header) = content.prev_sibling().and_downcast::<gtk4::Box>() {
            header.append(&indicator.area);
        }
        (content, indicator)
    }

    pub fn append(&self, child: &impl IsA<gtk4::Widget>) {
        self.column.append(child);
    }

    pub fn subsection(&self, parent: &impl Parent, title: &str, tip: &str) -> gtk4::Box {
        let (content, tip) = self.unkept_subsection(parent, title, tip);
        if let Some(tip) = tip {
            self.keep(tip);
        }
        content
    }

    pub fn unkept_subsection(
        &self,
        parent: &impl Parent,
        title: &str,
        tip: &str,
    ) -> (gtk4::Box, Option<Rc<Tooltip>>) {
        let subsection = gtk4::Box::new(gtk4::Orientation::Vertical, SUBSECTION_SPACING);
        subsection.set_margin_top(SUBSECTION_TOP);
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        if !title.is_empty() {
            let label = text::styled(title);
            text::set_color(&label, "colSubtext");
            label.set_margin_start(SUBSECTION_LABEL_START);
            header.append(&Centred::new(&label));
        }
        let tip = (!tip.is_empty()).then(|| {
            let info = text::symbol("info", pixel_size::LARGE as f64);
            text::set_color(&info, "colSubtext");
            info.set_cursor_from_name(Some("help"));
            header.append(&Centred::integral(&info));
            self.unkept_tip(&info, tip)
        });
        subsection.append(&header);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, SUBSECTION_SPACING);
        content.add_css_class(GROUP_CLASS);
        subsection.append(&content);
        parent.add(&subsection);
        (content, tip)
    }

    pub fn subsection_root(content: &gtk4::Box) -> gtk4::Widget {
        content
            .parent()
            .expect("a subsection's content sits in the subsection")
    }

    pub fn set_subsection_title(content: &gtk4::Box, title: &str) {
        let label = content
            .prev_sibling()
            .and_then(|header| header.first_child())
            .and_then(|centred| centred.first_child())
            .and_downcast::<gtk4::Label>();
        if let Some(label) = label {
            label.set_text(title);
        }
    }

    pub fn switch(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        toggled: impl Fn(bool) + 'static,
    ) -> Rc<ConfigSwitch> {
        let switch = ConfigSwitch::new(&self.theme, icon, label, toggled);
        switch.button.set_hexpand(true);
        parent.add(&switch.button);
        self.keep(switch.clone());
        switch
    }

    pub fn config_switch(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: bool,
    ) -> Rc<ConfigSwitch> {
        let switch = self.switch(parent, icon, label, move |on| {
            config::store_value(pointer, Value::Bool(on));
        });
        switch.bind(move || config::value_bool(pointer, default));
        self.refresh_on(pointer, &switch);
        self.track(pointer, Value::Bool(default), &switch.button);
        switch
    }

    pub fn refresh_on(&self, pointer: &str, switch: &Rc<ConfigSwitch>) {
        let switch = Rc::downgrade(switch);
        self.watch(pointer, move || {
            if let Some(switch) = switch.upgrade() {
                switch.refresh();
            }
        });
    }

    pub fn spin_row(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        spin: &Rc<SpinBox>,
    ) -> gtk4::Box {
        self.keep(spin.clone());
        Self::unkept_spin_row(parent, icon, label, spin)
    }

    pub fn unkept_spin_row(
        parent: &impl Parent,
        icon: &str,
        label: &str,
        spin: &Rc<SpinBox>,
    ) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
        row.set_margin_start(ROW_MARGIN);
        row.set_margin_end(ROW_MARGIN);
        if !icon.is_empty() {
            let symbol = text::symbol(icon, pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnSecondaryContainer");
            row.append(&Centred::integral(&symbol));
        }
        let name = text::styled(label);
        text::set_color(&name, "colOnSecondaryContainer");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let name_box = Centred::filling_width(&name);
        name_box.set_hexpand(true);
        row.append(&name_box);
        row.append(&spin.root);
        parent.add(&row);
        row
    }

    #[allow(clippy::too_many_arguments)]
    pub fn config_spin(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: i64,
        range: (i64, i64),
        step: i64,
    ) -> (gtk4::Box, Rc<SpinBox>) {
        let spin = SpinBox::new(&self.theme, range.0, range.1, step, 0);
        spin.set_value(config::value_i64(pointer, default));
        spin.connect_changed(move |value| config::store_value(pointer, Value::from(value)));
        self.watch(pointer, {
            let spin = Rc::downgrade(&spin);
            move || {
                if let Some(spin) = spin.upgrade() {
                    spin.set_value(config::value_i64(pointer, default));
                }
            }
        });
        let row = self.spin_row(parent, icon, label, &spin);
        self.track(pointer, Value::from(default), &row);
        (row, spin)
    }

    /// A spin box over a fractional setting, shown multiplied by `scale`.
    #[allow(clippy::too_many_arguments)]
    pub fn config_spin_scaled(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: f64,
        scale: f64,
        range: (i64, i64),
        step: i64,
    ) -> (gtk4::Box, Rc<SpinBox>) {
        let shown = move || (config::value_f64(pointer, default) * scale).round() as i64;
        let spin = SpinBox::new(&self.theme, range.0, range.1, step, 0);
        spin.set_value(shown());
        spin.connect_changed(move |value| {
            config::store_value(pointer, Value::from(value as f64 / scale));
        });
        self.watch(pointer, {
            let spin = Rc::downgrade(&spin);
            move || {
                if let Some(spin) = spin.upgrade() {
                    spin.set_value(shown());
                }
            }
        });
        let row = self.spin_row(parent, icon, label, &spin);
        self.track(pointer, Value::from(default), &row);
        (row, spin)
    }

    /// A spin box over a setting stored `factor` times larger than shown, such as seconds shown as minutes.
    #[allow(clippy::too_many_arguments)]
    pub fn config_spin_multiple(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: i64,
        factor: i64,
        range: (i64, i64),
        step: i64,
    ) -> (gtk4::Box, Rc<SpinBox>) {
        let shown =
            move || (config::value_i64(pointer, default) as f64 / factor as f64).round() as i64;
        let spin = SpinBox::new(&self.theme, range.0, range.1, step, 0);
        spin.set_value(shown());
        spin.connect_changed(move |value| {
            config::store_value(pointer, Value::from(value * factor))
        });
        self.watch(pointer, {
            let spin = Rc::downgrade(&spin);
            move || {
                if let Some(spin) = spin.upgrade() {
                    spin.set_value(shown());
                }
            }
        });
        let row = self.spin_row(parent, icon, label, &spin);
        self.track(pointer, Value::from(default), &row);
        (row, spin)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn config_slider(
        &self,
        parent: &impl Parent,
        icon: &str,
        label: &str,
        pointer: &'static str,
        default: f64,
        range: (f64, f64),
        stops: Vec<f64>,
    ) -> Rc<Slider> {
        let (slider, _) = slider_row(&self.theme, parent, icon, label, range);
        slider.set_stops(stops);
        let show = move |slider: &Slider| {
            let value = config::value_f64(pointer, default);
            slider.set(value);
            slider.set_tooltip(&format!("{}", value.round()));
        };
        show(&slider);
        slider.on_moved({
            let slider = Rc::downgrade(&slider);
            move |value| {
                if let Some(slider) = slider.upgrade() {
                    slider.set_tooltip(&format!("{}", value.round()));
                }
                config::store_value(pointer, Value::from(value.round() as i64));
            }
        });
        self.watch(pointer, {
            let slider = Rc::downgrade(&slider);
            move || {
                if let Some(slider) = slider.upgrade() {
                    show(&slider);
                }
            }
        });
        if let Some(row) = slider.area.parent() {
            self.track(pointer, Value::from(default), &row);
        }
        self.keep(slider.clone());
        slider
    }

    pub fn set_spin_row_enabled(row: &gtk4::Box, spin: &SpinBox, enabled: bool) {
        row.set_sensitive(enabled);
        let opacity = if enabled { 1.0 } else { DISABLED_OPACITY };
        let mut child = row.first_child();
        while let Some(widget) = child {
            if &widget != spin.root.upcast_ref::<gtk4::Widget>() {
                widget.set_opacity(opacity);
            }
            child = widget.next_sibling();
        }
        spin.set_enabled(enabled);
    }

    pub fn tools_notice(&self, parent: &impl Parent, tools: &[&Tool], effect: &str) -> bool {
        let missing = tools::missing(tools);
        if missing.is_empty() {
            return true;
        }
        self.notice(parent, "info", &tools::missing_message(&missing, effect));
        false
    }

    pub fn notice(&self, parent: &impl Parent, icon: &str, message: &str) -> gtk4::Box {
        let notice = gtk4::Box::new(gtk4::Orientation::Horizontal, NOTICE_SPACING);
        notice.add_css_class("settings-notice");
        let symbol = text::symbol(icon, pixel_size::HUGE as f64);
        text::set_color(&symbol, "colOnPrimaryContainer");
        let symbol = Centred::integral(&symbol);
        symbol.set_valign(gtk4::Align::Start);
        notice.append(&symbol);
        let label = text::styled(message);
        text::set_color(&label, "colOnPrimaryContainer");
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_wrap_mode(gtk4::pango::WrapMode::Word);
        label.set_hexpand(true);
        notice.append(&label);
        parent.add(&notice);
        notice
    }

    pub fn selection(
        &self,
        parent: &impl Parent,
        choices: Vec<Choice>,
        pointer: &'static str,
        default: Value,
        selected: impl Fn(Value) + 'static,
    ) -> Rc<Selection> {
        let tracked = default.clone();
        let current = move || config::value(pointer).unwrap_or_else(|| default.clone());
        let selection = self.selection_of(parent, choices, &[pointer], current, selected);
        self.track(pointer, tracked, &selection.root);
        selection
    }

    pub fn selection_of(
        &self,
        parent: &impl Parent,
        choices: Vec<Choice>,
        pointers: &[&str],
        current: impl Fn() -> Value + 'static,
        selected: impl Fn(Value) + 'static,
    ) -> Rc<Selection> {
        let selection = Selection::new(&self.theme, choices, selected);
        selection.set_current(&current());
        let current = Rc::new(current);
        for pointer in pointers {
            let selection = Rc::downgrade(&selection);
            let current = current.clone();
            self.watch(pointer, move || {
                if let Some(selection) = selection.upgrade() {
                    selection.set_current(&current());
                }
            });
        }
        parent.add(&selection.root);
        self.keep(selection.clone());
        selection
    }

    pub fn combo(&self, parent: &impl Parent, icon: &str) -> Rc<ComboBox> {
        let combo = ComboBox::new(&self.theme);
        combo.set_icon(icon);
        combo.button.set_hexpand(true);
        parent.add(&combo.button);
        self.keep(combo.clone());
        combo
    }

    pub fn link_row(
        &self,
        parent: &impl Parent,
        icon: &str,
        title: &str,
        subtitle: &str,
        action: impl Fn() + 'static,
    ) -> gtk4::Label {
        let button = RippleButton::new(&self.theme);
        button.set_radius(rounding::SMALL as f64);
        button.set_look(Look {
            background: |theme| theme.colors.col_layer2,
            ..Look::default()
        });
        button.set_size_request(-1, LINK_HEIGHT);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, LINK_SPACING);
        if !icon.is_empty() {
            let symbol = text::symbol(icon, pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnSecondaryContainer");
            row.append(&without_height(&Centred::integral(&symbol)));
        }
        let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        lines.set_hexpand(true);
        lines.set_valign(gtk4::Align::Center);
        let name = text::styled(title);
        text::set_color(&name, "colOnLayer2");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centred::filling_width(&name));
        let detail = text::styled_sized(subtitle, pixel_size::SMALLER);
        text::set_color(&detail, "colSubtext");
        detail.set_xalign(0.0);
        detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        detail.set_visible(!subtitle.is_empty());
        lines.append(&Centred::filling_width(&detail));
        row.append(&lines);
        let chevron = text::symbol("chevron_right", pixel_size::LARGER as f64);
        text::set_color(&chevron, "colSubtext");
        row.append(&without_height(&Centred::integral(&chevron)));
        button.set_content(&row, LINK_PADDING, 0);
        button.connect_clicked(move |_| action());
        parent.add(&button);
        detail
    }

    pub fn row(&self, parent: &impl Parent) -> Row {
        let row = Row::new(ROW_GAP);
        parent.add(&row);
        row
    }

    pub fn uniform_row(&self, parent: &impl Parent) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_GAP);
        row.set_homogeneous(true);
        parent.add(&row);
        row
    }

    pub fn text_field(
        &self,
        parent: &impl Parent,
        style: Style,
        placeholder: &str,
        current: impl Fn() -> String + 'static,
        committed: impl Fn(String) + 'static,
    ) -> Rc<TextField> {
        let field = TextField::new(&self.theme, style, placeholder);
        field.root.set_hexpand(true);
        field.connect_finished(committed);
        field.bind(current);
        parent.add(&field.root);
        self.keep(field.clone());
        field
    }

    pub fn secret_field(&self, parent: &impl Parent, placeholder: &str) -> Rc<TextField> {
        let field = TextField::secret(&self.theme, Style::Outlined, placeholder);
        field.root.set_hexpand(true);
        parent.add(&field.root);
        self.keep(field.clone());
        field
    }

    pub fn icon_button(
        &self,
        icon: &str,
        filled: bool,
        label: &str,
        action: impl Fn() + 'static,
    ) -> (RippleButton, gtk4::Label) {
        let (button, name) = controls::icon_button(&self.theme, icon, filled, label);
        button.set_halign(gtk4::Align::Start);
        button.connect_clicked(move |_| action());
        (button, name)
    }

    pub fn config_text(
        &self,
        parent: &impl Parent,
        style: Style,
        placeholder: &str,
        pointer: &'static str,
        default: &str,
    ) -> Rc<TextField> {
        let tracked = Value::from(default);
        let default = default.to_owned();
        let current = move || config::value_str(pointer).unwrap_or_else(|| default.clone());
        let field = self.text_field(parent, style, placeholder, current, move |text| {
            config::store_value(pointer, Value::from(text));
        });
        self.refresh_text_on(pointer, &field);
        self.track(pointer, tracked, &field.root);
        field
    }

    pub fn config_list(
        &self,
        parent: &impl Parent,
        placeholder: &str,
        pointer: &'static str,
        default: &'static [&'static str],
    ) -> Rc<TextField> {
        let current = move || {
            config::value(pointer)
                .and_then(|value| value.as_array().cloned())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| default.iter().map(|item| (*item).to_owned()).collect())
                .join(", ")
        };
        let field = self.text_field(parent, Style::Outlined, placeholder, current, move |text| {
            let items: Vec<String> = text
                .split(',')
                .map(|item| item.trim().to_owned())
                .filter(|item| !item.is_empty())
                .collect();
            config::store_value(pointer, Value::from(items));
        });
        self.refresh_text_on(pointer, &field);
        self.track(pointer, Value::from(default.to_vec()), &field.root);
        field
    }

    pub fn refresh_text_on(&self, pointer: &str, field: &Rc<TextField>) {
        let field = Rc::downgrade(field);
        self.watch(pointer, move || {
            if let Some(field) = field.upgrade() {
                field.refresh();
            }
        });
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
