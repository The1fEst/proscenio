use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::ui::anim::{EMPHASIZED_DECEL, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, transparentize};
use crate::ui::widgets::text::{self, Family};

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
            Editor::Secret(line) => line.text().to_string(),
        }
    }

    fn set_text(&self, text: &str) {
        match self {
            Editor::Wrapping(view) => view.buffer().set_text(text),
            Editor::Secret(line) => line.set_text(text),
        }
    }

    fn shown_text(&self) -> String {
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
    focused: Cell<bool>,
    hovered: Cell<bool>,
    float: Cell<Tween>,
    ticking: Cell<bool>,
    finished: RefCell<Option<Rc<dyn Fn(String)>>>,
    accepted: RefCell<Option<Rc<dyn Fn()>>>,
    changed: RefCell<Option<Rc<dyn Fn()>>>,
    current: RefCell<Option<Box<dyn Fn() -> String>>>,
    before_edit: RefCell<String>,
}

impl TextField {
    pub fn new(theme: &SharedTheme, style: Style, placeholder: &str) -> Rc<Self> {
        let view = gtk4::TextView::new();
        view.set_wrap_mode(gtk4::WrapMode::WordChar);
        view.set_accepts_tab(false);
        Self::build(theme, style, placeholder, Editor::Wrapping(view))
    }

    pub fn secret(theme: &SharedTheme, style: Style, placeholder: &str) -> Rc<Self> {
        let line = gtk4::Text::new();
        line.set_visibility(false);
        Self::build(theme, style, placeholder, Editor::Secret(line))
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
        let font = text::font(Family::Main, pixel_size::SMALL as f64, "wght=450");

        let root: Framed = glib::Object::new();
        view.set_parent(&root);
        if style == Style::Outlined {
            let measured = root.create_pango_layout(None);
            measured.set_font_description(Some(&font));
            measured.set_wrap(pango::WrapMode::WordChar);
            let shown = editor.clone();
            root.imp().text_height.replace(Some(Box::new(move |width| {
                let text = shown.shown_text();
                measured.set_text(if text.is_empty() { " " } else { text.as_str() });
                measured.set_width(width.max(1) * pango::SCALE);
                measured.pixel_size().1
            })));
        }
        if style == Style::Outlined {
            root.set_margin_top(LABEL_ROOM);
        }

        let label = root.create_pango_layout(Some(placeholder));
        label.set_font_description(Some(&font));

        let field = Rc::new(TextField {
            root: root.clone(),
            editor: editor.clone(),
            style,
            theme: theme.clone(),
            label,
            focused: Cell::new(false),
            hovered: Cell::new(false),
            float: Cell::new(Tween::new(0.0, FLOAT_MILLIS, EMPHASIZED_DECEL)),
            ticking: Cell::new(false),
            finished: RefCell::new(None),
            accepted: RefCell::new(None),
            changed: RefCell::new(None),
            current: RefCell::new(None),
            before_edit: RefCell::new(String::new()),
        });
        root.imp().draw.replace(Some(Box::new({
            let field = Rc::downgrade(&field);
            move |snapshot, width, height| {
                if let Some(field) = field.upgrade() {
                    field.draw(snapshot, width, height);
                }
            }
        })));

        let focus = gtk4::EventControllerFocus::new();
        focus.connect_enter({
            let field = Rc::downgrade(&field);
            move |_| {
                if let Some(field) = field.upgrade() {
                    field.focused.set(true);
                    field.before_edit.replace(field.text());
                    field.restyle();
                }
            }
        });
        focus.connect_leave({
            let field = Rc::downgrade(&field);
            move |_| {
                if let Some(field) = field.upgrade() {
                    field.focused.set(false);
                    field.finish();
                    field.restyle();
                }
            }
        });
        view.add_controller(focus);

        let motion = gtk4::EventControllerMotion::new();
        motion.connect_enter({
            let field = Rc::downgrade(&field);
            move |_, _, _| {
                if let Some(field) = field.upgrade() {
                    field.hovered.set(true);
                    field.root.queue_draw();
                }
            }
        });
        motion.connect_leave({
            let field = Rc::downgrade(&field);
            move |_| {
                if let Some(field) = field.upgrade() {
                    field.hovered.set(false);
                    field.root.queue_draw();
                }
            }
        });
        root.add_controller(motion);

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

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let field = Rc::downgrade(&field);
            move |_, key, _, _| {
                if !matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter) {
                    return glib::Propagation::Proceed;
                }
                if let Some(field) = field.upgrade() {
                    field.finish();
                    let accepted = field.accepted.borrow().clone();
                    if let Some(accepted) = accepted {
                        accepted();
                    }
                }
                glib::Propagation::Stop
            }
        });
        view.add_controller(keys);

        editor.connect_changed({
            let field = Rc::downgrade(&field);
            move || {
                let Some(field) = field.upgrade() else {
                    return;
                };
                field.restyle();
                field.root.queue_resize();
                let changed = field.changed.borrow().clone();
                if let Some(changed) = changed {
                    changed();
                }
            }
        });
        field.restyle();
        field
    }

    pub fn text(&self) -> String {
        self.editor.text()
    }

    pub fn set_text(&self, text: &str) {
        self.editor.set_text(text);
    }

    pub fn set_enabled(&self, enabled: bool) {
        let editor = self.editor.widget();
        editor.set_sensitive(enabled);
        if enabled {
            editor.remove_css_class("disabled");
        } else {
            editor.add_css_class("disabled");
        }
        self.root.queue_draw();
    }

    pub fn set_text_visible(&self, visible: bool) {
        if let Editor::Secret(line) = &self.editor {
            line.set_visibility(visible);
        }
    }

    pub fn grab_focus(&self) {
        self.editor.widget().grab_focus();
    }

    pub fn set_placeholder(&self, placeholder: &str) {
        self.label.set_text(placeholder);
        self.root.queue_draw();
    }

    pub fn connect_changed(&self, action: impl Fn() + 'static) {
        self.changed.replace(Some(Rc::new(action)));
    }

    pub fn bind(&self, current: impl Fn() -> String + 'static) {
        self.current.replace(Some(Box::new(current)));
        self.refresh();
    }

    pub fn refresh(&self) {
        if self.focused.get() {
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
        self.finished.replace(Some(Rc::new(action)));
    }

    pub fn connect_accepted(&self, action: impl Fn() + 'static) {
        self.accepted.replace(Some(Rc::new(action)));
    }

    fn finish(&self) {
        let text = self.text();
        let edited = *self.before_edit.borrow() != text;
        let current = self.current.borrow().as_ref().map(|current| current());
        if edited && current.as_deref() != Some(text.as_str()) {
            let finished = self.finished.borrow().clone();
            if let Some(finished) = finished {
                finished(text.clone());
            }
        }
        self.before_edit.replace(text);
        self.refresh();
    }

    fn floating(&self) -> bool {
        self.focused.get() || !self.text().is_empty()
    }

    fn restyle(self: &Rc<Self>) {
        let target = if self.floating() { 1.0 } else { 0.0 };
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
        if self.ticking.replace(true) {
            return;
        }
        let field = self.clone();
        self.root.add_tick_callback(move |frame, clock| {
            frame.queue_draw();
            if field.float.get().running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            field.ticking.set(false);
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
        let focused = self.focused.get();
        let (width, height) = (width as f64, height as f64);
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
                let line = if focused { 2.0 } else { 1.0 };
                let colour = if focused {
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
                outline(&cr, inset, inset, width - line, height - line, RADIUS, gap);
                source(&cr, colour);
                cr.set_line_width(line);
                let _ = cr.stroke();
            }
            Style::Filled => {
                top_rounded(&cr, 0.0, 0.0, width, height, RADIUS);
                source(&cr, theme.m3.surface);
                let _ = cr.fill();
                let line = if focused {
                    theme.m3.primary
                } else if self.hovered.get() {
                    theme.m3.outline
                } else {
                    theme.m3.outline_variant
                };
                cr.rectangle(0.0, height - 1.0, width, 1.0);
                source(&cr, line);
                let _ = cr.fill();
            }
        }
        drop(cr);
        let label_colour = if focused && float > 0.5 {
            theme.m3.primary
        } else {
            theme.m3.outline
        };
        let label_colour = if self.editor.widget().is_sensitive() || self.style == Style::Outlined {
            label_colour
        } else {
            transparentize(label_colour, DISABLED_FADE)
        };
        snapshot.save();
        snapshot.translate(&graphene::Point::new(left as f32, top as f32));
        snapshot.scale(scale as f32, scale as f32);
        snapshot.append_layout(&self.label, &label_colour);
        snapshot.restore();
    }
}

fn source(cr: &gtk4::cairo::Context, colour: RGBA) {
    cr.set_source_rgba(
        colour.red() as f64,
        colour.green() as f64,
        colour.blue() as f64,
        colour.alpha() as f64,
    );
}

fn outline(
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

fn top_rounded(cr: &gtk4::cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    cr.new_path();
    cr.move_to(x, y + height);
    cr.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
    cr.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
    cr.line_to(x + width, y + height);
    cr.close_path();
}
