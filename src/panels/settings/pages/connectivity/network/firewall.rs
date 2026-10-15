use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::core::tools;
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::platform::network::firewall::{self, Rule, State};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

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
    is_busy: Cell<bool>,
    problem: RefCell<Option<String>>,
    list: gtk4::Box,
    remove_tips: RefCell<Vec<Box<dyn Any>>>,
    last_rules: RefCell<Option<Vec<Rule>>>,
    on_changed: RefCell<Option<Rc<dyn Fn()>>>,
}

impl Firewall {
    fn announce(self: &Rc<Self>) {
        self.fill();
        let on_changed = self.on_changed.borrow().clone();
        if let Some(on_changed) = on_changed {
            on_changed();
        }
    }

    fn run(self: &Rc<Self>, call: impl Future<Output = Result<(), String>> + 'static) {
        if self.is_busy.replace(true) {
            return;
        }
        self.problem.replace(None);
        self.announce();
        let firewall = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = call.await;
            if let Some(firewall) = firewall.upgrade() {
                firewall.is_busy.set(false);
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
        if self.last_rules.borrow().as_ref() == Some(&rules) {
            return;
        }
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let mut remove_tips: Vec<Box<dyn Any>> = Vec::new();
        if rules.is_empty() {
            let empty_label = text::styled(&tr("No rules yet"));
            text::set_color(&empty_label, "colSubtext");
            empty_label.set_xalign(0.0);
            empty_label.set_margin_start(NOTE_START);
            self.list.append(&empty_label);
        }
        for rule in &rules {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, RULE_HEIGHT);
            let content = Row::new(RULE_SPACING);
            content.set_margin_start(RULE_START);
            content.set_margin_end(RULE_END);
            content.set_hexpand(true);
            let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            lines.set_hexpand(true);
            lines.set_valign(gtk4::Align::Center);
            let title = text::styled(&format_title(rule));
            text::set_color(&title, "colOnLayer2");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centered::filling_width(&title));
            let detail = text::styled_sized(&format_detail(rule), pixel_size::SMALLER);
            text::set_color(&detail, "colSubtext");
            detail.set_xalign(0.0);
            detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centered::filling_width(&detail));
            content.append(&lines);
            let remove_button = RippleButton::new(&page.theme);
            remove_button.set_radius(REMOVE_SIZE as f64 / 2.0);
            remove_button.set_size_request(REMOVE_SIZE, REMOVE_SIZE);
            remove_button.set_valign(gtk4::Align::Center);
            let symbol = text::symbol("remove", REMOVE_ICON);
            text::set_color(&symbol, "colOnLayer2");
            remove_button.set_content(&Centered::integral(&symbol), 0, 0);
            remove_button.connect_clicked({
                let firewall = Rc::downgrade(self);
                let rule = rule.clone();
                move |_| {
                    if let Some(firewall) = firewall.upgrade() {
                        let rule = rule.clone();
                        firewall.run(async move { firewall::delete(&rule).await });
                    }
                }
            });
            content.append(&remove_button);
            remove_tips.push(Box::new(page.unkept_tip(&remove_button, &tr("Remove"))));
            card.append(&content);
            self.list.append(&card);
        }
        self.remove_tips.replace(remove_tips);
        self.last_rules.replace(Some(rules));
    }
}

fn format_title(rule: &Rule) -> String {
    let action = firewall::ACTIONS
        .iter()
        .position(|action| *action == rule.action)
        .map(|index| tr(ACTION_NAMES[index]))
        .unwrap_or_else(|| rule.action.clone());
    let target = if !rule.application.is_empty() {
        rule.application.clone()
    } else if rule.port.is_empty() {
        tr("every port")
    } else if rule.protocol == "any" {
        rule.port.replace(':', "–")
    } else {
        format!("{}/{}", rule.port.replace(':', "–"), rule.protocol)
    };
    format!("{action} {target}")
}

fn format_detail(rule: &Rule) -> String {
    let source = if rule.from.is_empty() {
        tr("from anywhere")
    } else {
        trf("from %1", &[&rule.from])
    };
    if rule.comment.is_empty() {
        source
    } else {
        format!("{source} · {}", rule.comment)
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
        is_busy: Cell::new(false),
        problem: RefCell::new(None),
        list: list.clone(),
        remove_tips: RefCell::new(Vec::new()),
        last_rules: RefCell::new(None),
        on_changed: RefCell::new(None),
    });

    let firewall_switch = page.switch(&section, "shield", &tr("Firewall"), {
        let firewall = Rc::downgrade(&firewall);
        move |is_on| {
            if let Some(firewall) = firewall.upgrade() {
                let verb = if is_on { "enable" } else { "disable" };
                firewall.run(async move { firewall::request(vec![verb.to_owned()]).await });
            }
        }
    });
    firewall_switch.bind({
        let firewall = Rc::downgrade(&firewall);
        move || {
            firewall
                .upgrade()
                .is_some_and(|firewall| firewall.state.borrow().is_enabled)
        }
    });
    let incoming_section = page.subsection(
        &section,
        &tr("Incoming connections no rule allows"),
        &tr("Block drops them silently, Refuse tells the other side they were turned away"),
    );
    let policy_selection = page.selection_of(
        &incoming_section,
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
    let problem_label = text::styled("");
    text::set_color(&problem_label, "colError");
    problem_label.set_xalign(0.0);
    problem_label.set_wrap(true);
    problem_label.set_margin_start(NOTE_START);
    problem_label.set_visible(false);
    section.append(&problem_label);

    let rules_section = page.section("rule", &tr("Rules"));
    rules_section.append(&list);

    let add_section = page.section("add_moderator", &tr("Add a rule"));
    let selected_action = Rc::new(RefCell::new("allow".to_owned()));
    let action_group = page.subsection(
        &add_section,
        &tr("Action"),
        &tr("Limit allows a connection but refuses an address that opens six or more within 30 seconds"),
    );
    let action_selection = selection(
        &page,
        &action_group,
        choices(&ACTION_NAMES, &firewall::ACTIONS),
        &selected_action,
    );
    let selected_protocol = Rc::new(RefCell::new("tcp".to_owned()));
    let protocol_group = page.subsection(&add_section, &tr("Protocol"), "");
    let protocol_selection = selection(
        &page,
        &protocol_group,
        choices(&PROTOCOL_NAMES, &firewall::PROTOCOLS),
        &selected_protocol,
    );
    let add_field = |label: &str| {
        let field = TextField::new(&page.theme, Style::Outlined, label);
        field.root.set_hexpand(true);
        add_section.append(&field.root);
        page.keep(field.clone());
        field
    };
    let port_field = add_field(&tr("Port, range or list, such as 22, 6000:6007 or 80,443"));
    let from_field = add_field(&tr("From (empty is anywhere), such as 192.168.1.0/24"));
    let comment_field = add_field(&tr("Comment"));
    let (add_button, _) = page.icon_button("add", true, &tr("Add rule"), {
        let firewall = Rc::downgrade(&firewall);
        let (port_field, from_field, comment_field) = (
            Rc::downgrade(&port_field),
            Rc::downgrade(&from_field),
            Rc::downgrade(&comment_field),
        );
        let (selected_action, selected_protocol) =
            (selected_action.clone(), selected_protocol.clone());
        move || {
            let (Some(firewall), Some(port_field), Some(from_field), Some(comment_field)) = (
                firewall.upgrade(),
                port_field.upgrade(),
                from_field.upgrade(),
                comment_field.upgrade(),
            ) else {
                return;
            };
            let rule = Rule {
                action: selected_action.borrow().clone(),
                protocol: selected_protocol.borrow().clone(),
                port: normalize_ports(&port_field.text()),
                from: from_field.text().trim().to_owned(),
                application: String::new(),
                comment: comment_field.text().trim().to_owned(),
            };
            if let Err(message) = firewall::check(&rule) {
                firewall.problem.replace(Some(message));
                firewall.announce();
                return;
            }
            let fields = [&port_field, &from_field, &comment_field]
                .map(|field| (Rc::downgrade(field), field.text()));
            firewall.run(async move {
                let result = firewall::add(&rule).await;
                if result.is_ok() {
                    for (field, submitted_text) in &fields {
                        if let Some(field) = field
                            .upgrade()
                            .filter(|field| field.text() == *submitted_text)
                        {
                            field.set_text("");
                        }
                    }
                }
                result
            });
        }
    });
    add_button.set_margin_top(BUTTON_TOP);
    add_section.append(&add_button);
    let add_problem_label = text::styled("");
    text::set_color(&add_problem_label, "colError");
    add_problem_label.set_xalign(0.0);
    add_problem_label.set_wrap(true);
    add_problem_label.set_margin_start(NOTE_START);
    add_problem_label.set_visible(false);
    add_section.append(&add_problem_label);

    firewall.on_changed.replace(Some(Rc::new({
        let firewall = Rc::downgrade(&firewall);
        let policy_selection = Rc::downgrade(&policy_selection);
        move || {
            let (Some(firewall), Some(policy_selection)) =
                (firewall.upgrade(), policy_selection.upgrade())
            else {
                return;
            };
            let is_busy = firewall.is_busy.get();
            firewall_switch.refresh();
            firewall_switch.set_enabled(!is_busy);
            add_button.set_sensitive(!is_busy);
            policy_selection.set_current(&Value::from(firewall.state.borrow().incoming.clone()));
            let message = firewall.problem.borrow().clone();
            for label in [&problem_label, &add_problem_label] {
                label.set_visible(message.is_some());
                label.set_text(message.as_deref().unwrap_or_default());
            }
        }
    })));
    firewall.announce();
    page.keep(action_selection);
    page.keep(protocol_selection);
    page.keep(firewall);
    page
}

fn selection(
    page: &Page,
    parent: &gtk4::Box,
    choices: Vec<Choice>,
    selected_value: &Rc<RefCell<String>>,
) -> Rc<Selection> {
    let selection_slot: Rc<RefCell<Option<Weak<Selection>>>> = Rc::default();
    let selection = Selection::new(&page.theme, choices, {
        let selected_value = selected_value.clone();
        let selection_slot = selection_slot.clone();
        move |value| {
            selected_value.replace(value.as_str().unwrap_or_default().to_owned());
            if let Some(selection) = selection_slot.borrow().as_ref().and_then(Weak::upgrade) {
                selection.set_current(&value);
            }
        }
    });
    selection.set_current(&Value::from(selected_value.borrow().clone()));
    selection_slot.replace(Some(Rc::downgrade(&selection)));
    parent.append(&selection.root);
    selection
}

fn normalize_ports(text: &str) -> String {
    text.split(',')
        .map(|part| part.split(':').map(str::trim).collect::<Vec<_>>().join(":"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_lose_the_spaces_around_separators_but_not_between_numbers() {
        assert_eq!(
            normalize_ports(" 80, 443 ,6000 : 6007 "),
            "80,443,6000:6007"
        );
        assert_eq!(normalize_ports("80 443"), "80 443");
        let rule = Rule {
            action: "allow".to_owned(),
            protocol: "tcp".to_owned(),
            port: normalize_ports("22 80"),
            ..Rule::default()
        };
        assert!(firewall::check(&rule).is_err());
    }
}
