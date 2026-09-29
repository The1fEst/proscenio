use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::Config;
use crate::ui::theme::pixel_size;

#[derive(Clone, Copy)]
pub enum Family {
    Main,
    Title,
    Monospace,
    Reading,
    Expressive,
    Nerd,
    Material,
}

struct Families {
    main: String,
    title: String,
    monospace: String,
    reading: String,
    expressive: String,
    nerd: String,
}

thread_local! {
    static FAMILIES: RefCell<Option<Families>> = const { RefCell::new(None) };
}

pub fn init(config: &Config) {
    FAMILIES.with(|families| {
        families.replace(Some(Families {
            main: config.font_main.clone(),
            title: config.font_title.clone(),
            monospace: config.font_monospace.clone(),
            reading: config.font_reading.clone(),
            expressive: config.font_expressive.clone(),
            nerd: config.font_nerd.clone(),
        }));
    });
}

pub fn family(which: Family) -> String {
    if let Family::Material = which {
        return "Material Symbols Rounded".to_owned();
    }
    FAMILIES.with(|families| {
        let families = families.borrow();
        let Some(families) = families.as_ref() else {
            return String::new();
        };
        match which {
            Family::Main => families.main.clone(),
            Family::Title => families.title.clone(),
            Family::Monospace => families.monospace.clone(),
            Family::Reading => families.reading.clone(),
            Family::Expressive => families.expressive.clone(),
            Family::Nerd => families.nerd.clone(),
            Family::Material => unreachable!(),
        }
    })
}

pub fn kdeglobals(group: &str, key: &str) -> Option<String> {
    let path = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join(".config/kdeglobals");
    let text = std::fs::read_to_string(path).ok()?;
    let header = format!("[{group}]");
    let prefix = format!("{key}=");
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with('[') {
            inside = line == header;
            continue;
        }
        if let Some(value) = line.strip_prefix(&prefix).filter(|_| inside) {
            return Some(value.to_owned());
        }
    }
    None
}

pub fn application_font() -> (String, i32) {
    let Some(value) = kdeglobals("General", "font") else {
        return ("sans-serif".to_owned(), 400);
    };
    let fields: Vec<&str> = value.split(',').collect();
    let weight = fields
        .get(4)
        .and_then(|field| field.parse().ok())
        .unwrap_or(400);
    (fields[0].to_owned(), weight)
}

/// Qt's default font size in pixels, which Qt's text metrics use when given only a family.
pub fn application_pixel_size() -> f64 {
    const DEFAULT_POINTS: f64 = 10.0;
    let points = kdeglobals("General", "font")
        .and_then(|value| value.split(',').nth(1)?.parse().ok())
        .unwrap_or(DEFAULT_POINTS);
    points * 96.0 / 72.0
}

pub fn icon_theme() -> String {
    kdeglobals("Icons", "Theme").unwrap_or_else(|| "breeze".to_owned())
}

const DEFAULT_OPTICAL_SIZE: &str = "opsz=18";

pub fn font(which: Family, size: f64, variations: &str) -> pango::FontDescription {
    let mut font = pango::FontDescription::new();
    font.set_family(&family(which));
    font.set_absolute_size(size * pango::SCALE as f64);
    let variations = match which {
        Family::Material => variations.to_owned(),
        _ if variations.contains("opsz") => variations.to_owned(),
        _ if variations.is_empty() => DEFAULT_OPTICAL_SIZE.to_owned(),
        _ => format!("{variations},{DEFAULT_OPTICAL_SIZE}"),
    };
    if !variations.is_empty() {
        font.set_variations(Some(&variations));
    }
    font
}

const ROLE: &str = "proscenio-font-role";

pub fn set_font(label: &gtk4::Label, which: Family, size: f64, variations: &str) {
    let font = font(which, size, variations);
    let attributes = label.attributes().unwrap_or_default();
    attributes.change(pango::AttrFontDesc::new(&font));
    label.set_attributes(Some(&attributes));
    unsafe {
        label.set_data(ROLE, which);
    }
}

pub fn refont(root: &gtk4::Widget) {
    if let Some(label) = root.downcast_ref::<gtk4::Label>()
        && let Some(which) = unsafe { label.data::<Family>(ROLE).map(|role| *role.as_ref()) }
        && let Some(attributes) = label.attributes()
    {
        for attribute in attributes.attributes() {
            if let Some(font) = attribute.downcast_ref::<pango::AttrFontDesc>() {
                let mut desc = font.desc();
                desc.set_family(&family(which));
                attributes.change(pango::AttrFontDesc::new(&desc));
            }
        }
        label.set_attributes(Some(&attributes));
    }
    root.queue_draw();
    let mut child = root.first_child();
    while let Some(current) = child {
        refont(&current);
        child = current.next_sibling();
    }
}

pub fn set_application_font(label: &gtk4::Label, size: f64) {
    let (family, weight) = application_font();
    let mut font = pango::FontDescription::new();
    font.set_family(&family);
    font.set_absolute_size(size * pango::SCALE as f64);
    font.set_variations(Some(&format!("wght={weight},{DEFAULT_OPTICAL_SIZE}")));
    let attributes = label.attributes().unwrap_or_default();
    attributes.change(pango::AttrFontDesc::new(&font));
    label.set_attributes(Some(&attributes));
}

pub fn set_color(label: &impl IsA<gtk4::Widget>, token: &str) {
    for class in label.css_classes() {
        if class.starts_with("fg-") {
            label.remove_css_class(&class);
        }
    }
    label.add_css_class(&format!("fg-{token}"));
}

pub fn styled(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("styled-text");
    set_font(&label, Family::Main, pixel_size::SMALL as f64, "wght=450");
    set_color(&label, "m3onBackground");
    label
}

pub fn styled_sized(text: &str, size: i32) -> gtk4::Label {
    let label = styled(text);
    set_font(&label, Family::Main, size as f64, "wght=450");
    label
}

const CHANGE_MICROS: f64 = 150_000.0;
const CHANGE_DISTANCE: f64 = 6.0;

mod imp {
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use std::cell::Cell;

    #[derive(Default)]
    pub struct Shift {
        pub offset: Cell<f32>,
        pub sideways: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Shift {
        const NAME: &'static str = "ProscenioShift";
        type Type = super::Shift;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Shift {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Shift {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.obj().first_child() {
                Some(child) => child.measure(orientation, for_size),
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                let offset = self.offset.get();
                let point = if self.sideways.get() {
                    gtk4::graphene::Point::new(offset, 0.0)
                } else {
                    gtk4::graphene::Point::new(0.0, offset)
                };
                let shift = gtk4::gsk::Transform::new().translate(&point);
                child.allocate(width, height, baseline, Some(shift));
            }
        }
    }
}

gtk4::glib::wrapper! {
    pub struct Shift(ObjectSubclass<imp::Shift>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Shift {
    pub fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let shift: Shift = gtk4::glib::Object::new();
        child.set_parent(&shift);
        shift
    }

    pub fn set_offset(&self, offset: f32) {
        use gtk4::subclass::prelude::ObjectSubclassIsExt;
        self.imp().offset.set(offset);
        self.queue_allocate();
    }

    pub fn set_sideways(&self, sideways: bool) {
        use gtk4::subclass::prelude::ObjectSubclassIsExt;
        self.imp().sideways.set(sideways);
        self.queue_allocate();
    }
}

pub fn shifted(child: &impl IsA<gtk4::Widget>, offset: f32) -> gtk4::Widget {
    let shift = Shift::new(child);
    shift.set_offset(offset);
    shift.upcast()
}

pub type Changing = (gtk4::Widget, Rc<dyn Fn(&str)>);

pub fn animate_change(label: &gtk4::Label) -> Changing {
    changing(label, Shift::new(label))
}

pub fn animate_change_sideways(label: &gtk4::Label) -> Changing {
    let holder = Shift::new(&crate::ui::widgets::centred::Centred::filling_width(label));
    {
        use gtk4::subclass::prelude::ObjectSubclassIsExt;
        holder.imp().sideways.set(true);
    }
    changing(label, holder)
}

fn changing(label: &gtk4::Label, holder: Shift) -> Changing {
    let running = Rc::new(Cell::new(false));
    let pending: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let set: Rc<dyn Fn(&str)> = Rc::new({
        let label = label.clone();
        let holder = holder.clone();
        move |value: &str| {
            if label.text() == value && pending.borrow().is_none() {
                return;
            }
            pending.replace(Some(value.to_owned()));
            if running.replace(true) {
                return;
            }
            let label = label.clone();
            let pending = pending.clone();
            let running = running.clone();
            let started = Cell::new(0i64);
            holder.add_tick_callback(move |holder, clock| {
                let now = clock.frame_time();
                if started.get() == 0 {
                    started.set(now);
                }
                let elapsed = (now - started.get()) as f64;
                let half = std::f64::consts::FRAC_PI_2;
                if elapsed < CHANGE_MICROS {
                    let part = elapsed / CHANGE_MICROS;
                    let eased = 1.0 - (part * half).cos();
                    holder.set_offset((-CHANGE_DISTANCE * eased) as f32);
                    label.set_opacity(1.0 - eased);
                    return gtk4::glib::ControlFlow::Continue;
                }
                if let Some(next) = pending.borrow_mut().take() {
                    label.set_text(&next);
                }
                let part = ((elapsed - CHANGE_MICROS) / CHANGE_MICROS).min(1.0);
                let eased = (part * half).sin();
                holder.set_offset((CHANGE_DISTANCE * (1.0 - eased)) as f32);
                label.set_opacity(eased);
                if part < 1.0 {
                    return gtk4::glib::ControlFlow::Continue;
                }
                running.set(false);
                gtk4::glib::ControlFlow::Break
            });
        }
    });
    (holder.upcast(), set)
}

pub fn set_symbol_font(label: &gtk4::Label, size: f64, fill: f64) {
    let fill = (fill * 10.0).round() / 10.0;
    set_symbol_font_weighted(label, size, fill, 400.0 + 200.0 * fill);
}

pub fn set_symbol_font_weighted(label: &gtk4::Label, size: f64, fill: f64, weight: f64) {
    let attributes = label.attributes().unwrap_or_default();
    attributes.change(pango::AttrFontDesc::new(&symbol_font(size, fill, weight)));
    label.set_attributes(Some(&attributes));
}

pub fn symbol_font(size: f64, fill: f64, weight: f64) -> pango::FontDescription {
    let fill = (fill * 10.0).round() / 10.0;
    font(
        Family::Material,
        size,
        &format!("FILL={fill},opsz={size},wght={weight}"),
    )
}

pub fn fill_motion(label: &gtk4::Label, size: f64, start: f64) -> Rc<dyn Fn(f64)> {
    let tween = Rc::new(Cell::new(crate::ui::anim::Tween::new(
        start,
        200.0,
        crate::ui::anim::EXPRESSIVE_EFFECTS,
    )));
    let ticking = Rc::new(Cell::new(false));
    let label = label.clone();
    Rc::new(move |target: f64| {
        let now = label
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(gtk4::glib::monotonic_time);
        let mut next = tween.get();
        next.retarget(target, now);
        tween.set(next);
        if !label.is_mapped() {
            let mut settled = tween.get();
            settled.jump(target);
            tween.set(settled);
            set_symbol_font(&label, size, target);
            return;
        }
        if ticking.replace(true) {
            return;
        }
        let tween = tween.clone();
        let ticking = ticking.clone();
        label.add_tick_callback(move |label, clock| {
            let now = clock.frame_time();
            let current = tween.get();
            set_symbol_font(label, size, current.value(now));
            if current.running(now) {
                return gtk4::glib::ControlFlow::Continue;
            }
            ticking.set(false);
            gtk4::glib::ControlFlow::Break
        });
    })
}

pub fn symbol(name: &str, size: f64) -> gtk4::Label {
    symbol_filled(name, size, 0.0)
}

pub fn symbol_filled(name: &str, size: f64, fill: f64) -> gtk4::Label {
    let label = gtk4::Label::new(Some(name));
    label.add_css_class("material-symbol");
    set_symbol_font(&label, size, fill);
    set_color(&label, "m3onBackground");
    label
}
