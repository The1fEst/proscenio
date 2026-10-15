use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::platform::security::pam;
use crate::ui::anim::{EMPHASIZED_DECEL, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, transparentize};
use crate::ui::widgets::display::text::{self, Family};

const HEIGHT: i32 = 56;
const LABEL_ROOM: i32 = 9;
const PADDING: i32 = 16;
const FLOAT_SCALE: f64 = 0.8;
const OUTLINED_PADDING: i32 = 15;
const TEXT_VIEW_CURSOR: i32 = 1;
const GAP: f64 = 4.0;
const RADIUS: f64 = 4.0;
const FLOAT_MILLIS: f64 = 150.0;
const FILLED_TEXT_TOP: i32 = 26;
const FILLED_LABEL_TOP: f64 = 10.0;
const DISABLED_FADE: f32 = 0.62;
const OUTLINED_LABEL_RAISE: f64 = 1.0;

type Draw = Box<dyn Fn(&gtk4::Snapshot, f32, f32)>;
type TextHeight = Box<dyn Fn(i32) -> i32>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Framed {
        pub draw: RefCell<Option<Draw>>,
        pub text_height: RefCell<Option<TextHeight>>,
    }

    impl Framed {
        fn text_height(&self, width: i32) -> Option<i32> {
            self.text_height
                .borrow()
                .as_ref()
                .map(|measure| measure(width - OUTLINED_PADDING * 2 - TEXT_VIEW_CURSOR))
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Framed {
        const NAME: &'static str = "ProscenioTextField";
        type Type = super::Framed;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Framed {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Framed {
        fn request_mode(&self) -> gtk4::SizeRequestMode {
            gtk4::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            let (minimum, natural, _, _) = child.measure(orientation, for_size);
            if orientation == gtk4::Orientation::Horizontal {
                return (minimum, natural, -1, -1);
            }
            let text = Some(for_size)
                .filter(|width| *width >= 0)
                .and_then(|width| self.text_height(width))
                .map(|text| text + PADDING)
                .unwrap_or(0);
            (minimum.max(HEIGHT), natural.max(HEIGHT).max(text), -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let Some(child) = self.obj().first_child() else {
                return;
            };
            let Some(text) = self.text_height(width).filter(|text| *text < height) else {
                child.allocate(width, height, -1, None);
                return;
            };
            let top = (height - text + 1) / 2;
            let place =
                gtk4::gsk::Transform::new().translate(&graphene::Point::new(0.0, top as f32));
            child.allocate(width, text, -1, Some(place));
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            if let Some(draw) = self.draw.borrow().as_ref() {
                draw(snapshot, obj.width() as f32, obj.height() as f32);
            }
            if let Some(child) = obj.first_child() {
                obj.snapshot_child(&child, snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct Framed(ObjectSubclass<imp::Framed>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Outlined,
    Filled,
}

#[derive(Clone)]
enum Editor {
    Wrapping(gtk4::TextView),
    Secret(gtk4::Text),
}

impl Editor {
    fn widget(&self) -> gtk4::Widget {
        match self {
            Editor::Wrapping(view) => view.clone().upcast(),
            Editor::Secret(line) => line.clone().upcast(),
        }
    }

    fn text(&self) -> String {
        match self {
            Editor::Wrapping(view) => {
                let buffer = view.buffer();
                buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), false)
                    .to_string()
            }
            Editor::Secret(line) => pam::copy_text(&line.buffer()),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Editor::Wrapping(_) => self.text().is_empty(),
            Editor::Secret(line) => line.text_length() == 0,
        }
    }

    fn set_text(&self, text: &str) {
        match self {
            Editor::Wrapping(view) => view.buffer().set_text(text),
            Editor::Secret(line) => line.set_text(text),
        }
    }

    fn text_for_layout(&self) -> String {
        match self {
            Editor::Wrapping(_) => self.text(),
            Editor::Secret(_) => String::new(),
        }
    }

    fn connect_changed(&self, action: impl Fn() + 'static) {
        match self {
            Editor::Wrapping(view) => {
                view.buffer().connect_changed(move |_| action());
            }
            Editor::Secret(line) => {
                line.connect_changed(move |_| action());
            }
        }
    }
}

pub struct TextField {
    pub root: Framed,
    editor: Editor,
    style: Style,
    theme: SharedTheme,
    label: pango::Layout,
    is_focused: Cell<bool>,
    is_hovered: Cell<bool>,
    float: Cell<Tween>,
    is_ticking: Cell<bool>,
    on_finished: RefCell<Option<Rc<dyn Fn(String)>>>,
    on_accepted: RefCell<Option<Rc<dyn Fn()>>>,
    on_changed: RefCell<Option<Rc<dyn Fn()>>>,
    current: RefCell<Option<Box<dyn Fn() -> String>>>,
    original_text: RefCell<String>,
}

impl TextField {
    pub fn new(theme: &SharedTheme, style: Style, placeholder: &str) -> Rc<Self> {
        let view = gtk4::TextView::new();
        view.set_wrap_mode(gtk4::WrapMode::WordChar);
        view.set_accepts_tab(false);
        Self::build(theme, style, placeholder, Editor::Wrapping(view))
    }

    pub fn secret(theme: &SharedTheme, style: Style, placeholder: &str) -> Rc<Self> {
        let buffer = gtk4::PasswordEntryBuffer::new();
        Self::secret_with_buffer(theme, style, placeholder, buffer.upcast_ref())
    }

    pub fn secret_with_buffer(
        theme: &SharedTheme,
        style: Style,
        placeholder: &str,
        buffer: &gtk4::EntryBuffer,
    ) -> Rc<Self> {
        let line = gtk4::Text::with_buffer(buffer);
        line.set_visibility(false);
        Self::build(theme, style, placeholder, Editor::Secret(line))
    }

    fn build(theme: &SharedTheme, style: Style, placeholder: &str, editor: Editor) -> Rc<Self> {
        let view = editor.widget();
        view.add_css_class("material-text");
        view.set_hexpand(true);
        let padding = match style {
            Style::Outlined => OUTLINED_PADDING,
            Style::Filled => PADDING,
        };
        view.set_margin_start(padding);
        view.set_margin_end(padding);
        if style == Style::Filled {
            view.set_margin_top(FILLED_TEXT_TOP);
            view.set_margin_bottom(PADDING / 2);
        }
        let root: Framed = glib::Object::new();
        view.set_parent(&root);
        if style == Style::Outlined {
            let layout = root.create_pango_layout(None);
            layout.set_wrap(pango::WrapMode::WordChar);
            let editor_handle = editor.clone();
            root.imp().text_height.replace(Some(Box::new(move |width| {
                layout.set_font_description(Some(&field_font()));
                let text = editor_handle.text_for_layout();
                layout.set_text(if text.is_empty() { " " } else { text.as_str() });
                layout.set_width(width.max(1) * pango::SCALE);
                layout.pixel_size().1
            })));
        }
        if style == Style::Outlined {
            root.set_margin_top(LABEL_ROOM);
        }

        let label = root.create_pango_layout(Some(placeholder));

        let field = Rc::new(TextField {
            root: root.clone(),
            editor: editor.clone(),
            style,
            theme: theme.clone(),
            label,
            is_focused: Cell::new(false),
            is_hovered: Cell::new(false),
            float: Cell::new(Tween::new(0.0, FLOAT_MILLIS, EMPHASIZED_DECEL)),
            is_ticking: Cell::new(false),
            on_finished: RefCell::new(None),
            on_accepted: RefCell::new(None),
            on_changed: RefCell::new(None),
            current: RefCell::new(None),
            original_text: RefCell::new(String::new()),
        });
        root.imp().draw.replace(Some(Box::new({
            let field = Rc::downgrade(&field);
            move |snapshot, width, height| {
                if let Some(field) = field.upgrade() {
                    field.draw(snapshot, width, height);
                }
            }
        })));

        let focus_controller = gtk4::EventControllerFocus::new();
        focus_controller.connect_enter({
            let field = Rc::downgrade(&field);
            move |_| {
                if let Some(field) = field.upgrade() {
                    field.is_focused.set(true);
                    if field.on_finished.borrow().is_some() {
                        field.remember(field.text());
                    }
                    field.restyle();
                }
            }
        });
        focus_controller.connect_leave({
            let field = Rc::downgrade(&field);
            move |_| {
                if let Some(field) = field.upgrade() {
                    field.is_focused.set(false);
                    field.finish();
                    field.restyle();
                }
            }
        });
        view.add_controller(focus_controller);

        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_enter({
            let field = Rc::downgrade(&field);
            move |_, _, _| {
                if let Some(field) = field.upgrade() {
                    field.is_hovered.set(true);
                    field.root.queue_draw();
                }
            }
        });
        motion_controller.connect_leave({
            let field = Rc::downgrade(&field);
            move |_| {
                if let Some(field) = field.upgrade() {
                    field.is_hovered.set(false);
                    field.root.queue_draw();
                }
            }
        });
        root.add_controller(motion_controller);

        let click = gtk4::GestureClick::new();
        click.connect_pressed({
            let view = view.downgrade();
            move |_, _, _, _| {
                if let Some(view) = view.upgrade() {
                    view.grab_focus();
                }
            }
        });
        root.add_controller(click);

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let field = Rc::downgrade(&field);
            move |_, key, _, _| {
                if !matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter) {
                    return glib::Propagation::Proceed;
                }
                if let Some(field) = field.upgrade() {
                    field.finish();
                    let on_accepted = field.on_accepted.borrow().clone();
                    if let Some(on_accepted) = on_accepted {
                        on_accepted();
                    }
                }
                glib::Propagation::Stop
            }
        });
        view.add_controller(key_controller);

        editor.connect_changed({
            let field = Rc::downgrade(&field);
            move || {
                let Some(field) = field.upgrade() else {
                    return;
                };
                field.restyle();
                field.root.queue_resize();
                let on_changed = field.on_changed.borrow().clone();
                if let Some(on_changed) = on_changed {
                    on_changed();
                }
            }
        });
        field.restyle();
        field
    }

    pub fn text(&self) -> String {
        self.editor.text()
    }

    pub fn is_empty(&self) -> bool {
        self.editor.is_empty()
    }

    pub fn set_text(&self, text: &str) {
        self.editor.set_text(text);
    }

    pub fn set_enabled(&self, is_enabled: bool) {
        let editor = self.editor.widget();
        editor.set_sensitive(is_enabled);
        if is_enabled {
            editor.remove_css_class("disabled");
        } else {
            editor.add_css_class("disabled");
        }
        self.root.queue_draw();
    }

    pub fn set_text_visible(&self, is_visible: bool) {
        if let Editor::Secret(line) = &self.editor {
            line.set_visibility(is_visible);
        }
    }

    pub fn grab_focus(&self) {
        self.editor.widget().grab_focus();
    }

    pub fn type_from(&self, key_controller: &gtk4::EventControllerKey) -> bool {
        let editor = self.editor.widget();
        editor.grab_focus();
        match &self.editor {
            Editor::Wrapping(view) => {
                let buffer = view.buffer();
                buffer.place_cursor(&buffer.end_iter());
            }
            Editor::Secret(line) => line.set_position(-1),
        }
        key_controller.forward(&editor)
    }

    pub fn set_placeholder(&self, placeholder: &str) {
        self.label.set_text(placeholder);
        self.root.queue_draw();
    }

    pub fn connect_changed(&self, action: impl Fn() + 'static) {
        self.on_changed.replace(Some(Rc::new(action)));
    }

    pub fn bind(&self, current: impl Fn() -> String + 'static) {
        self.current.replace(Some(Box::new(current)));
        self.refresh();
    }

    pub fn refresh(&self) {
        if self.is_focused.get() {
            return;
        }
        let Some(current) = self.current.borrow().as_ref().map(|current| current()) else {
            return;
        };
        if self.text() != current {
            self.editor.set_text(&current);
        }
    }

    pub fn connect_finished(&self, action: impl Fn(String) + 'static) {
        self.on_finished.replace(Some(Rc::new(action)));
    }

    pub fn connect_accepted(&self, action: impl Fn() + 'static) {
        self.on_accepted.replace(Some(Rc::new(action)));
    }

    fn finish(&self) {
        let on_finished = self.on_finished.borrow().clone();
        if let Some(on_finished) = on_finished {
            let text = self.text();
            let is_edited = *self.original_text.borrow() != text;
            let current = self.current.borrow().as_ref().map(|current| current());
            if is_edited && current.as_deref() != Some(text.as_str()) {
                on_finished(text.clone());
            }
            self.remember(text);
        }
        self.refresh();
    }

    fn remember(&self, text: String) {
        let previous = self.original_text.replace(text);
        pam::wipe(&mut previous.into_bytes());
    }

    fn is_floating(&self) -> bool {
        self.is_focused.get() || !self.editor.is_empty()
    }

    fn restyle(self: &Rc<Self>) {
        let target = if self.is_floating() { 1.0 } else { 0.0 };
        let mut tween = self.float.get();
        if !self.root.is_mapped() {
            tween.jump(target);
            self.float.set(tween);
            self.root.queue_draw();
            return;
        }
        let now = self
            .root
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        if (tween.target() - target).abs() > f64::EPSILON {
            tween.retarget(target, now);
            self.float.set(tween);
        }
        self.root.queue_draw();
        if self.is_ticking.replace(true) {
            return;
        }
        let field = Rc::downgrade(self);
        self.root.add_tick_callback(move |frame, clock| {
            let Some(field) = field.upgrade() else {
                return glib::ControlFlow::Break;
            };
            frame.queue_draw();
            if field.float.get().is_running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            field.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn draw(&self, snapshot: &gtk4::Snapshot, width: f32, height: f32) {
        let theme = self.theme.borrow();
        let now = self
            .root
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let float = self.float.get().value(now);
        let is_focused = self.is_focused.get();
        let (width, height) = (width as f64, height as f64);
        self.label.set_font_description(Some(&field_font()));
        let (_, logical) = self.label.extents();
        let label_width = logical.width() as f64 / pango::SCALE as f64;
        let label_height = logical.height() as f64 / pango::SCALE as f64;
        let scale = 1.0 + (FLOAT_SCALE - 1.0) * float;
        let resting_top = match self.style {
            Style::Outlined => (height - LABEL_ROOM as f64 - label_height) / 2.0,
            Style::Filled => (height - label_height) / 2.0,
        };
        let floating_top = match self.style {
            Style::Outlined => -label_height * FLOAT_SCALE / 2.0 - OUTLINED_LABEL_RAISE,
            Style::Filled => FILLED_LABEL_TOP,
        };
        let top = resting_top + (floating_top - resting_top) * float;
        let left = PADDING as f64;

        let cr = snapshot.append_cairo(&graphene::Rect::new(
            0.0,
            -(LABEL_ROOM as f32),
            width as f32,
            height as f32 + LABEL_ROOM as f32,
        ));
        match self.style {
            Style::Outlined => {
                let line = if is_focused { 2.0 } else { 1.0 };
                let color = if is_focused {
                    theme.m3.primary
                } else {
                    theme.qt_hint()
                };
                let inset = line / 2.0;
                let gap = if float > 0.0 {
                    Some((left - GAP, left + label_width * scale + GAP))
                } else {
                    None
                };
                trace_outline(&cr, inset, inset, width - line, height - line, RADIUS, gap);
                set_source(&cr, color);
                cr.set_line_width(line);
                let _ = cr.stroke();
            }
            Style::Filled => {
                trace_top_rounded(&cr, 0.0, 0.0, width, height, RADIUS);
                set_source(&cr, theme.m3.surface);
                let _ = cr.fill();
                let line = if is_focused {
                    theme.m3.primary
                } else if self.is_hovered.get() {
                    theme.m3.outline
                } else {
                    theme.m3.outline_variant
                };
                cr.rectangle(0.0, height - 1.0, width, 1.0);
                set_source(&cr, line);
                let _ = cr.fill();
            }
        }
        drop(cr);
        let label_color = if is_focused && float > 0.5 {
            theme.m3.primary
        } else {
            theme.m3.outline
        };
        let label_color = if self.editor.widget().is_sensitive() || self.style == Style::Outlined {
            label_color
        } else {
            transparentize(label_color, DISABLED_FADE)
        };
        snapshot.save();
        snapshot.translate(&graphene::Point::new(left as f32, top as f32));
        snapshot.scale(scale as f32, scale as f32);
        snapshot.append_layout(&self.label, &label_color);
        snapshot.restore();
    }
}

impl Drop for TextField {
    fn drop(&mut self) {
        self.remember(String::new());
    }
}

fn field_font() -> pango::FontDescription {
    text::font(Family::Main, pixel_size::SMALL as f64, "wght=450")
}

fn set_source(cr: &gtk4::cairo::Context, color: RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}

fn trace_outline(
    cr: &gtk4::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
    gap: Option<(f64, f64)>,
) {
    let (right, bottom) = (x + width, y + height);
    cr.new_path();
    match gap {
        Some((_, end)) => cr.move_to(end.min(right - radius), y),
        None => cr.move_to(x + radius, y),
    }
    cr.arc(right - radius, y + radius, radius, -PI / 2.0, 0.0);
    cr.arc(right - radius, bottom - radius, radius, 0.0, PI / 2.0);
    cr.arc(x + radius, bottom - radius, radius, PI / 2.0, PI);
    cr.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
    match gap {
        Some((start, _)) => cr.line_to(start.max(x + radius), y),
        None => cr.close_path(),
    }
}

fn trace_top_rounded(
    cr: &gtk4::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    cr.new_path();
    cr.move_to(x, y + height);
    cr.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
    cr.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
    cr.line_to(x + width, y + height);
    cr.close_path();
}
