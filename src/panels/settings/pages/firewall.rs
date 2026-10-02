use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::core::tools;
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::platform::firewall::{self, Rule, State};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;

const NOTE_START: i32 = 8;
const RULE_HEIGHT: i32 = 48;
const RULE_START: i32 = 12;
const RULE_END: i32 = 6;
const RULE_SPACING: i32 = 10;
const REMOVE_SIZE: i32 = 32;
const REMOVE_ICON: f64 = 20.0;
const BUTTON_TOP: i32 = 4;
const ACTION_NAMES: [&str; 4] = ["Allow", "Deny", "Reject", "Limit"];
const PROTOCOL_NAMES: [&str; 3] = ["TCP and UDP", "TCP", "UDP"];
const POLICY_NAMES: [&str; 3] = ["Block", "Refuse", "Allow"];

struct Firewall {
    page: Weak<Page>,
    state: RefCell<State>,
    busy: Cell<bool>,
    problem: RefCell<Option<String>>,
    list: gtk4::Box,
    rows: RefCell<Vec<Box<dyn Any>>>,
    shown: RefCell<Option<Vec<Rule>>>,
    changed: RefCell<Option<Rc<dyn Fn()>>>,
}

impl Firewall {
    fn announce(self: &Rc<Self>) {
        self.fill();
        let changed = self.changed.borrow().clone();
        if let Some(changed) = changed {
            changed();
        }
    }

    fn run(self: &Rc<Self>, call: impl Future<Output = Result<(), String>> + 'static) {
        if self.busy.replace(true) {
            return;
        }
        self.problem.replace(None);
        self.announce();
        let firewall = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = call.await;
            if let Some(firewall) = firewall.upgrade() {
                firewall.busy.set(false);
                firewall.problem.replace(result.err());
                firewall.state.replace(firewall::read());
                firewall.announce();
            }
        });
    }

    fn fill(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let rules = self.state.borrow().rules.clone();
        if self.shown.borrow().as_ref() == Some(&rules) {
            return;
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let mut rows: Vec<Box<dyn Any>> = Vec::new();
        if rules.is_empty() {
            let empty = text::styled(&tr("No rules yet"));
            text::set_color(&empty, "colSubtext");
            empty.set_xalign(0.0);
            empty.set_margin_start(NOTE_START);
            self.list.append(&empty);
        }
        for rule in &rules {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, RULE_HEIGHT);
            let inside = Row::new(RULE_SPACING);
            inside.set_margin_start(RULE_START);
            inside.set_margin_end(RULE_END);
            inside.set_hexpand(true);
            let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            lines.set_hexpand(true);
            lines.set_valign(gtk4::Align::Center);
            let title = text::styled(&title(rule));
            text::set_color(&title, "colOnLayer2");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centred::filling_width(&title));
            let detail = text::styled_sized(&detail(rule), pixel_size::SMALLER);
            text::set_color(&detail, "colSubtext");
            detail.set_xalign(0.0);
            detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centred::filling_width(&detail));
            inside.append(&lines);
            let remove = RippleButton::new(&page.theme);
            remove.set_radius(REMOVE_SIZE as f64 / 2.0);
            remove.set_size_request(REMOVE_SIZE, REMOVE_SIZE);
            remove.set_valign(gtk4::Align::Center);
            let symbol = text::symbol("remove", REMOVE_ICON);
            text::set_color(&symbol, "colOnLayer2");
            remove.set_content(&Centred::integral(&symbol), 0, 0);
            remove.connect_clicked({
                let firewall = Rc::downgrade(self);
                let rule = rule.clone();
                move |_| {
                    if let Some(firewall) = firewall.upgrade() {
                        let rule = rule.clone();
                        firewall.run(async move { firewall::delete(&rule).await });
                    }
                }
            });
            inside.append(&remove);
            rows.push(Box::new(page.unkept_tip(&remove, &tr("Remove"))));
            card.append(&inside);
            self.list.append(&card);
        }
        self.rows.replace(rows);
        self.shown.replace(Some(rules));
    }
}

fn title(rule: &Rule) -> String {
    let action = firewall::ACTIONS
        .iter()
        .position(|action| *action == rule.action)
        .map(|index| tr(ACTION_NAMES[index]))
        .unwrap_or_else(|| rule.action.clone());
    let what = if !rule.application.is_empty() {
        rule.application.clone()
    } else if rule.port.is_empty() {
        tr("every port")
    } else if rule.protocol == "any" {
        rule.port.replace(':', "–")
    } else {
        format!("{}/{}", rule.port.replace(':', "–"), rule.protocol)
    };
    format!("{action} {what}")
}

fn detail(rule: &Rule) -> String {
    let from = if rule.from.is_empty() {
        tr("from anywhere")
    } else {
        trf("from %1", &[&rule.from])
    };
    if rule.comment.is_empty() {
        from
    } else {
        format!("{from} · {}", rule.comment)
    }
}

fn choices(names: &[&str], values: &[&str]) -> Vec<Choice> {
    names
        .iter()
        .zip(values)
        .map(|(name, value)| Choice {
            label: tr(name),
            icon: "",
            value: Value::from(*value),
        })
        .collect()
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let section = page.section("", "");
    if !page.tools_notice(
        &section,
        &[&tools::UFW],
        &tr("the firewall cannot be set up"),
    ) {
        return page;
    }
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let firewall = Rc::new(Firewall {
        page: Rc::downgrade(&page),
        state: RefCell::new(firewall::read()),
        busy: Cell::new(false),
        problem: RefCell::new(None),
        list: list.clone(),
        rows: RefCell::new(Vec::new()),
        shown: RefCell::new(None),
        changed: RefCell::new(None),
    });

    let switch = page.switch(&section, "shield", &tr("Firewall"), {
        let firewall = Rc::downgrade(&firewall);
        move |on| {
            if let Some(firewall) = firewall.upgrade() {
                let verb = if on { "enable" } else { "disable" };
                firewall.run(async move { firewall::request(vec![verb.to_owned()]).await });
            }
        }
    });
    switch.bind({
        let firewall = Rc::downgrade(&firewall);
        move || {
            firewall
                .upgrade()
                .is_some_and(|firewall| firewall.state.borrow().enabled)
        }
    });
    let incoming = page.subsection(
        &section,
        &tr("Incoming connections no rule allows"),
        &tr("Block drops them silently, Refuse tells the other side they were turned away"),
    );
    let policy = page.selection_of(
        &incoming,
        choices(&POLICY_NAMES, &firewall::POLICIES),
        &[],
        {
            let firewall = Rc::downgrade(&firewall);
            move || {
                Value::from(
                    firewall
                        .upgrade()
                        .map(|firewall| firewall.state.borrow().incoming.clone())
                        .unwrap_or_default(),
                )
            }
        },
        {
            let firewall = Rc::downgrade(&firewall);
            move |value| {
                if let Some(firewall) = firewall.upgrade() {
                    let policy = value.as_str().unwrap_or("deny").to_owned();
                    firewall.run(async move {
                        firewall::request(vec!["default".to_owned(), policy]).await
                    });
                }
            }
        },
    );
    let problem = text::styled("");
    text::set_color(&problem, "colError");
    problem.set_xalign(0.0);
    problem.set_wrap(true);
    problem.set_margin_start(NOTE_START);
    problem.set_visible(false);
    section.append(&problem);

    let rules = page.section("rule", &tr("Rules"));
    rules.append(&list);

    let adding = page.section("add_moderator", &tr("Add a rule"));
    let action = Rc::new(RefCell::new("allow".to_owned()));
    let action_group = page.subsection(
        &adding,
        &tr("Action"),
        &tr("Limit allows a connection but refuses an address that opens six or more within 30 seconds"),
    );
    let actions = selection(
        &page,
        &action_group,
        choices(&ACTION_NAMES, &firewall::ACTIONS),
        &action,
    );
    let protocol = Rc::new(RefCell::new("tcp".to_owned()));
    let protocol_group = page.subsection(&adding, &tr("Protocol"), "");
    let protocols = selection(
        &page,
        &protocol_group,
        choices(&PROTOCOL_NAMES, &firewall::PROTOCOLS),
        &protocol,
    );
    let field = |label: &str| {
        let field = TextField::new(&page.theme, Style::Outlined, label);
        field.root.set_hexpand(true);
        adding.append(&field.root);
        page.keep(field.clone());
        field
    };
    let port = field(&tr("Port, range or list, such as 22, 6000:6007 or 80,443"));
    let from = field(&tr("From (empty is anywhere), such as 192.168.1.0/24"));
    let comment = field(&tr("Comment"));
    let (add, _) = page.icon_button("add", true, &tr("Add rule"), {
        let firewall = Rc::downgrade(&firewall);
        let (port, from, comment) = (
            Rc::downgrade(&port),
            Rc::downgrade(&from),
            Rc::downgrade(&comment),
        );
        let (action, protocol) = (action.clone(), protocol.clone());
        move || {
            let (Some(firewall), Some(port), Some(from), Some(comment)) = (
                firewall.upgrade(),
                port.upgrade(),
                from.upgrade(),
                comment.upgrade(),
            ) else {
                return;
            };
            let rule = Rule {
                action: action.borrow().clone(),
                protocol: protocol.borrow().clone(),
                port: port.text().split_whitespace().collect::<String>(),
                from: from.text().trim().to_owned(),
                application: String::new(),
                comment: comment.text().trim().to_owned(),
            };
            if let Err(message) = firewall::check(&rule) {
                firewall.problem.replace(Some(message));
                firewall.announce();
                return;
            }
            for field in [&port, &from, &comment] {
                field.set_text("");
            }
            firewall.run(async move { firewall::add(&rule).await });
        }
    });
    add.set_margin_top(BUTTON_TOP);
    adding.append(&add);
    let add_problem = text::styled("");
    text::set_color(&add_problem, "colError");
    add_problem.set_xalign(0.0);
    add_problem.set_wrap(true);
    add_problem.set_margin_start(NOTE_START);
    add_problem.set_visible(false);
    adding.append(&add_problem);

    firewall.changed.replace(Some(Rc::new({
        let firewall = Rc::downgrade(&firewall);
        let policy = Rc::downgrade(&policy);
        move || {
            let (Some(firewall), Some(policy)) = (firewall.upgrade(), policy.upgrade()) else {
                return;
            };
            let busy = firewall.busy.get();
            switch.refresh();
            switch.set_enabled(!busy);
            policy.set_current(&Value::from(firewall.state.borrow().incoming.clone()));
            let message = firewall.problem.borrow().clone();
            for label in [&problem, &add_problem] {
                label.set_visible(message.is_some());
                label.set_text(message.as_deref().unwrap_or_default());
            }
        }
    })));
    firewall.announce();
    page.keep(actions);
    page.keep(protocols);
    page.keep(firewall);
    page
}

fn selection(
    page: &Page,
    parent: &gtk4::Box,
    choices: Vec<Choice>,
    chosen: &Rc<RefCell<String>>,
) -> Rc<Selection> {
    let holder: Rc<RefCell<Option<Weak<Selection>>>> = Rc::default();
    let selection = Selection::new(&page.theme, choices, {
        let chosen = chosen.clone();
        let holder = holder.clone();
        move |value| {
            chosen.replace(value.as_str().unwrap_or_default().to_owned());
            if let Some(selection) = holder.borrow().as_ref().and_then(Weak::upgrade) {
                selection.set_current(&value);
            }
        }
    });
    selection.set_current(&Value::from(chosen.borrow().clone()));
    holder.replace(Some(Rc::downgrade(&selection)));
    parent.append(&selection.root);
    selection
}
