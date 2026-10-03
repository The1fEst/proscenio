use gtk4::prelude::*;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::platform::hypr;
use crate::platform::windowrules::{self, Rule};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;

const RULE_HEIGHT: i32 = 48;
const RULE_START: i32 = 12;
const RULE_END: i32 = 6;
const RULE_SPACING: i32 = 10;
const REMOVE_SIZE: i32 = 32;
const REMOVE_ICON: f64 = 20.0;
const EMPTY_START: i32 = 8;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fixed(&'static str),
    Percent,
    Text,
}

const FLAG: Kind = Kind::Fixed("true");

const RULE_KINDS: [(&str, &str, Kind); 12] = [
    ("Always floating", "float", FLAG),
    ("Always tiled", "tile", FLAG),
    ("Pinned to every workspace", "pin", FLAG),
    ("Opens fullscreen", "fullscreen", FLAG),
    ("No blur behind it", "no_blur", FLAG),
    ("No shadow", "no_shadow", FLAG),
    ("Square corners", "rounding", Kind::Fixed("0")),
    ("Draw without waiting for the screen", "immediate", FLAG),
    ("Takes focus when it asks", "focus_on_activate", FLAG),
    (
        "Never takes focus when it asks",
        "focus_on_activate",
        Kind::Fixed("false"),
    ),
    ("Opacity (%)", "opacity", Kind::Percent),
    ("Opens on workspace", "workspace", Kind::Text),
];

fn summary(rule: &Rule) -> String {
    let Some((name, _, kind)) = RULE_KINDS.iter().find(|(_, known, kind)| {
        *known == rule.rule && !matches!(kind, Kind::Fixed(value) if *value != rule.value)
    }) else {
        return format!("{} = {}", rule.rule, rule.value);
    };
    match kind {
        Kind::Fixed(_) => tr(name),
        Kind::Percent => format!(
            "{}: {}%",
            tr(name).replace(" (%)", ""),
            (rule.value.parse::<f64>().unwrap_or(f64::NAN) * 100.0).round()
        ),
        Kind::Text => format!("{}: {}", tr(name), rule.value),
    }
}

fn running_classes() -> Vec<String> {
    let mut seen: Vec<String> = hypr::json("clients")
        .and_then(|clients| clients.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|client| client.get("class")?.as_str().map(str::to_owned))
        .filter(|class| !class.is_empty())
        .collect();
    seen.sort();
    seen.dedup();
    seen
}

struct Rules {
    page: Weak<Page>,
    list: gtk4::Box,
    empty: Centred,
    rows: RefCell<Vec<Box<dyn Any>>>,
}

impl Rules {
    fn reload(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let rules = windowrules::read();
        self.empty.set_visible(rules.is_empty());
        let mut child = self.empty.next_sibling();
        while let Some(widget) = child {
            child = widget.next_sibling();
            self.list.remove(&widget);
        }
        let mut rows: Vec<Box<dyn Any>> = Vec::new();
        for rule in rules {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, RULE_HEIGHT);
            let inside = Row::new(RULE_SPACING);
            inside.set_margin_start(RULE_START);
            inside.set_margin_end(RULE_END);
            inside.set_hexpand(true);
            let class = text::styled(&rule.class);
            text::set_color(&class, "colOnLayer2");
            class.set_xalign(0.0);
            class.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let class = Centred::filling_width(&class);
            class.set_hexpand(true);
            inside.append(&class);
            let described = text::styled_sized(&summary(&rule), pixel_size::SMALLER);
            text::set_color(&described, "colSubtext");
            inside.append(&Centred::new(&described));
            let remove = RippleButton::new(&page.theme);
            remove.set_radius(REMOVE_SIZE as f64 / 2.0);
            remove.set_size_request(REMOVE_SIZE, REMOVE_SIZE);
            remove.set_valign(gtk4::Align::Center);
            let symbol = text::symbol("remove", REMOVE_ICON);
            text::set_color(&symbol, "colOnLayer2");
            remove.set_content(&Centred::integral(&symbol), 0, 0);
            remove.connect_clicked({
                let rules = Rc::downgrade(self);
                let (class, name) = (rule.class.clone(), rule.rule.clone());
                move |_| {
                    let _ = windowrules::remove(&class, &name);
                    hypr::request("reload");
                    if let Some(rules) = rules.upgrade() {
                        rules.reload();
                    }
                }
            });
            inside.append(&remove);
            rows.push(Box::new(page.unkept_tip(&remove, &tr("Remove"))));
            card.append(&inside);
            self.list.append(&card);
        }
        self.rows.replace(rows);
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let windows = page.section("", "");
    let listed = page.subsection(&windows, &tr("What each application's windows do"), "");
    let empty = text::styled(&tr("No rules yet"));
    text::set_color(&empty, "colSubtext");
    let empty = Centred::new(&empty);
    empty.set_halign(gtk4::Align::Start);
    empty.set_margin_start(EMPTY_START);
    listed.append(&empty);
    let rules = Rc::new(Rules {
        page: Rc::downgrade(&page),
        list: listed,
        empty,
        rows: RefCell::new(Vec::new()),
    });
    rules.reload();

    let adding = page.subsection(
        &windows,
        &tr("Add a rule"),
        &tr(
            "The application is matched by its window class, which is what hyprctl clients calls class",
        ),
    );
    let class_field = TextField::new(&page.theme, Style::Outlined, &tr("Window class"));
    class_field.root.set_hexpand(true);
    adding.append(&class_field.root);
    let classes = Rc::new(RefCell::new(Vec::new()));
    let running = page.combo(&adding, "window");
    let fill = Rc::new({
        let running = Rc::downgrade(&running);
        let classes = classes.clone();
        move || {
            let Some(running) = running.upgrade() else {
                return;
            };
            let fresh = running_classes();
            running.set_items(&fresh, 0);
            running.button.set_visible(!fresh.is_empty());
            classes.replace(fresh);
        }
    });
    fill();
    page.keep(context.services.events.subscribe({
        let fill = fill.clone();
        move |event, _| {
            if matches!(event, "openwindow" | "closewindow") {
                fill();
            }
        }
    }));
    running.connect_activated({
        let class_field = Rc::downgrade(&class_field);
        let running = Rc::downgrade(&running);
        move |index| {
            let classes = classes.borrow();
            if let Some(running) = running.upgrade() {
                running.set_items(&classes, index as i32);
            }
            if let (Some(field), Some(class)) = (class_field.upgrade(), classes.get(index)) {
                field.set_text(class);
            }
        }
    });
    let chosen = Rc::new(Cell::new(0usize));
    let kinds = page.combo(&adding, "rule");
    let names: Vec<String> = RULE_KINDS.iter().map(|(name, _, _)| tr(name)).collect();
    kinds.set_items(&names, 0);
    let value_field = TextField::new(&page.theme, Style::Outlined, "");
    value_field.root.set_hexpand(true);
    adding.append(&value_field.root);
    let show_kind = {
        let value_field = Rc::downgrade(&value_field);
        let chosen = chosen.clone();
        move || {
            let Some(field) = value_field.upgrade() else {
                return;
            };
            let kind = RULE_KINDS[chosen.get()].2;
            field.root.set_visible(!matches!(kind, Kind::Fixed(_)));
            field.set_placeholder(&if kind == Kind::Percent {
                tr("100")
            } else {
                tr("e.g. 3 or special:magic")
            });
        }
    };
    show_kind();
    kinds.connect_activated({
        let kinds = Rc::downgrade(&kinds);
        let chosen = chosen.clone();
        let names = names.clone();
        move |index| {
            chosen.set(index);
            if let Some(kinds) = kinds.upgrade() {
                kinds.set_items(&names, index as i32);
            }
            show_kind();
        }
    });
    let (add, _) = page.icon_button("add", true, &tr("Add rule"), {
        let class_field = Rc::downgrade(&class_field);
        let value_field = Rc::downgrade(&value_field);
        let rules = Rc::downgrade(&rules);
        move || {
            let (Some(class_field), Some(value_field)) =
                (class_field.upgrade(), value_field.upgrade())
            else {
                return;
            };
            let class = class_field.text();
            let (_, rule, kind) = RULE_KINDS[chosen.get()];
            let value = match kind {
                Kind::Fixed(value) => value.to_owned(),
                Kind::Percent => {
                    let percent = value_field
                        .text()
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|percent| *percent != 0.0 && percent.is_finite())
                        .unwrap_or(100.0)
                        .clamp(1.0, 100.0);
                    format!("{}", percent / 100.0)
                }
                Kind::Text => value_field.text(),
            };
            if class.is_empty() {
                return;
            }
            let _ = windowrules::add(&class, rule, &value);
            hypr::request("reload");
            class_field.set_text("");
            value_field.set_text("");
            if let Some(rules) = rules.upgrade() {
                rules.reload();
            }
        }
    });
    adding.append(&add);
    add.set_sensitive(false);
    class_field.connect_changed({
        let class_field = Rc::downgrade(&class_field);
        let add = add.clone();
        move || {
            if let Some(field) = class_field.upgrade() {
                add.set_sensitive(!field.text().is_empty());
            }
        }
    });
    page.keep(class_field);
    page.keep(value_field);
    page.keep(rules);
    page
}
