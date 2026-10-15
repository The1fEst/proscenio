use gtk4::prelude::*;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::platform::hyprland::hypr;
use crate::platform::hyprland::windowrules::{self, Rule};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

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
    let mut classes: Vec<String> = hypr::json("clients")
        .and_then(|clients| clients.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|client| client.get("class")?.as_str().map(str::to_owned))
        .filter(|class| !class.is_empty())
        .collect();
    classes.sort();
    classes.dedup();
    classes
}

struct Rules {
    page: Weak<Page>,
    list: gtk4::Box,
    empty_box: Centered,
    remove_tips: RefCell<Vec<Box<dyn Any>>>,
}

impl Rules {
    fn reload(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let rules = windowrules::read();
        self.empty_box.set_visible(rules.is_empty());
        let mut child = self.empty_box.next_sibling();
        while let Some(widget) = child {
            child = widget.next_sibling();
            self.list.remove(&widget);
        }
        let mut remove_tips: Vec<Box<dyn Any>> = Vec::new();
        for rule in rules {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, RULE_HEIGHT);
            let content = Row::new(RULE_SPACING);
            content.set_margin_start(RULE_START);
            content.set_margin_end(RULE_END);
            content.set_hexpand(true);
            let class_label = text::styled(&rule.class);
            text::set_color(&class_label, "colOnLayer2");
            class_label.set_xalign(0.0);
            class_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let class_box = Centered::filling_width(&class_label);
            class_box.set_hexpand(true);
            content.append(&class_box);
            let summary_label = text::styled_sized(&summary(&rule), pixel_size::SMALLER);
            text::set_color(&summary_label, "colSubtext");
            content.append(&Centered::new(&summary_label));
            let remove_button = RippleButton::new(&page.theme);
            remove_button.set_radius(REMOVE_SIZE as f64 / 2.0);
            remove_button.set_size_request(REMOVE_SIZE, REMOVE_SIZE);
            remove_button.set_valign(gtk4::Align::Center);
            let symbol = text::symbol("remove", REMOVE_ICON);
            text::set_color(&symbol, "colOnLayer2");
            remove_button.set_content(&Centered::integral(&symbol), 0, 0);
            remove_button.connect_clicked({
                let rules = Rc::downgrade(self);
                let (class, rule_name) = (rule.class.clone(), rule.rule.clone());
                move |_| {
                    let _ = windowrules::remove(&class, &rule_name);
                    hypr::request("reload");
                    if let Some(rules) = rules.upgrade() {
                        rules.reload();
                    }
                }
            });
            content.append(&remove_button);
            remove_tips.push(Box::new(page.unkept_tip(&remove_button, &tr("Remove"))));
            card.append(&content);
            self.list.append(&card);
        }
        self.remove_tips.replace(remove_tips);
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let windows_section = page.section("", "");
    let rules_section = page.subsection(
        &windows_section,
        &tr("What each application's windows do"),
        "",
    );
    let empty_label = text::styled(&tr("No rules yet"));
    text::set_color(&empty_label, "colSubtext");
    let empty_box = Centered::new(&empty_label);
    empty_box.set_halign(gtk4::Align::Start);
    empty_box.set_margin_start(EMPTY_START);
    rules_section.append(&empty_box);
    let rules = Rc::new(Rules {
        page: Rc::downgrade(&page),
        list: rules_section,
        empty_box,
        remove_tips: RefCell::new(Vec::new()),
    });
    rules.reload();

    let add_section = page.subsection(
        &windows_section,
        &tr("Add a rule"),
        &tr(
            "The application is matched by its window class, which is what hyprctl clients calls class",
        ),
    );
    let class_field = TextField::new(&page.theme, Style::Outlined, &tr("Window class"));
    class_field.root.set_hexpand(true);
    add_section.append(&class_field.root);
    let classes = Rc::new(RefCell::new(Vec::new()));
    let running_combo = page.combo(&add_section, "window");
    let fill = Rc::new({
        let running_combo = Rc::downgrade(&running_combo);
        let classes = classes.clone();
        move || {
            let Some(running_combo) = running_combo.upgrade() else {
                return;
            };
            let current_classes = running_classes();
            running_combo.set_items(&current_classes, 0);
            running_combo
                .button
                .set_visible(!current_classes.is_empty());
            classes.replace(current_classes);
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
    running_combo.connect_activated({
        let class_field = Rc::downgrade(&class_field);
        let running_combo = Rc::downgrade(&running_combo);
        move |index| {
            let classes = classes.borrow();
            if let Some(running_combo) = running_combo.upgrade() {
                running_combo.set_items(&classes, index as i32);
            }
            if let (Some(class_field), Some(class)) = (class_field.upgrade(), classes.get(index)) {
                class_field.set_text(class);
            }
        }
    });
    let kind_index = Rc::new(Cell::new(0usize));
    let kind_combo = page.combo(&add_section, "rule");
    let names: Vec<String> = RULE_KINDS.iter().map(|(name, _, _)| tr(name)).collect();
    kind_combo.set_items(&names, 0);
    let value_field = TextField::new(&page.theme, Style::Outlined, "");
    value_field.root.set_hexpand(true);
    add_section.append(&value_field.root);
    let show_kind = {
        let value_field = Rc::downgrade(&value_field);
        let kind_index = kind_index.clone();
        move || {
            let Some(value_field) = value_field.upgrade() else {
                return;
            };
            let kind = RULE_KINDS[kind_index.get()].2;
            value_field
                .root
                .set_visible(!matches!(kind, Kind::Fixed(_)));
            value_field.set_placeholder(&if kind == Kind::Percent {
                tr("100")
            } else {
                tr("e.g. 3 or special:magic")
            });
        }
    };
    show_kind();
    kind_combo.connect_activated({
        let kind_combo = Rc::downgrade(&kind_combo);
        let kind_index = kind_index.clone();
        let names = names.clone();
        move |index| {
            kind_index.set(index);
            if let Some(kind_combo) = kind_combo.upgrade() {
                kind_combo.set_items(&names, index as i32);
            }
            show_kind();
        }
    });
    let (add_button, _) = page.icon_button("add", true, &tr("Add rule"), {
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
            let (_, rule, kind) = RULE_KINDS[kind_index.get()];
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
    add_section.append(&add_button);
    add_button.set_sensitive(false);
    class_field.connect_changed({
        let class_field = Rc::downgrade(&class_field);
        let add_button = add_button.clone();
        move || {
            if let Some(class_field) = class_field.upgrade() {
                add_button.set_sensitive(!class_field.text().is_empty());
            }
        }
    });
    page.keep(class_field);
    page.keep(value_field);
    page.keep(rules);
    page
}
