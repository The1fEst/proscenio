pub mod eap;
mod fields;
mod interfaces;
pub mod ipv4;
pub mod ipv6;
mod persistence;
mod startup;
mod vpn;
mod wireguard;

use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use self::fields::show_note;
use self::persistence::{Actions, Operation};
use self::startup::{Start, start};
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::connectivity::network::check_manager_running;
use crate::platform::network::nmprofile::ipconfig::Family;
use crate::platform::network::nmprofile::{
    self, BOND, BRIDGE, Profile, VLAN, VPN, WIRED, WIREGUARD, WIRELESS,
};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::overlay::windowdialog::WindowDialog;

pub const NEW_WIRED: &str = "new:802-3-ethernet";
pub const NEW_WIREGUARD: &str = "new:wireguard";
pub const NEW_ENTERPRISE: &str = "new:enterprise:";
pub const NEW_OPENVPN: &str = "new:openvpn";
pub const NEW_VLAN: &str = "new:vlan";
pub const NEW_BRIDGE: &str = "new:bridge";
pub const NEW_BOND: &str = "new:bond";
const BOND_MODES: [(&str, &str); 7] = [
    ("Active backup", "active-backup"),
    ("Round robin", "balance-rr"),
    ("XOR", "balance-xor"),
    ("Broadcast", "broadcast"),
    ("802.3ad (LACP)", "802.3ad"),
    ("Adaptive transmit", "balance-tlb"),
    ("Adaptive load balancing", "balance-alb"),
];
const NEW: &str = "new:";
const NOTE_START: i32 = 8;
const BUTTON_SPACING: i32 = 5;
const MTU_MAX: i64 = 9000;
const PORT_MAX: i64 = 65535;

pub(super) const EAP_METHODS: [(&str, &str); 3] = [
    ("Protected EAP (PEAP)", "peap"),
    ("Tunneled TLS (TTLS)", "ttls"),
    ("TLS", "tls"),
];
const PEAP_INNER: [(&str, &str); 3] = [("MSCHAPv2", "mschapv2"), ("GTC", "gtc"), ("MD5", "md5")];
const TTLS_INNER: [(&str, &str); 4] = [
    ("PAP", "pap"),
    ("MSCHAPv2", "mschapv2"),
    ("MSCHAP", "mschap"),
    ("CHAP", "chap"),
];
pub(super) const IPV4_METHODS: [(&str, &str); 5] = [
    ("Automatic", "auto"),
    ("Manual", "manual"),
    ("Link-local only", "link-local"),
    ("Shared with other computers", "shared"),
    ("Disabled", "disabled"),
];
pub(super) const IPV6_METHODS: [(&str, &str); 6] = [
    ("Automatic", "auto"),
    ("Automatic, DHCP only", "dhcp"),
    ("Manual", "manual"),
    ("Link-local only", "link-local"),
    ("Shared with other computers", "shared"),
    ("Disabled", "disabled"),
];
pub(super) const PRIVACY: [(&str, i32); 4] = [
    ("Default", -1),
    ("Off", 0),
    ("Prefer the fixed address", 1),
    ("Prefer a temporary address", 2),
];

#[derive(Clone)]
struct InvalidText {
    message: String,
    text: String,
}

#[derive(Clone, Copy)]
pub(super) enum Part {
    Main,
    Sub(fn(&Rc<Editor>, &Page)),
}

pub(super) struct Editor {
    argument: Option<String>,
    is_started: Cell<bool>,
    part: Cell<Part>,
    page: RefCell<Weak<Page>>,
    form: RefCell<gtk4::Box>,
    status: RefCell<glib::WeakRef<gtk4::Box>>,
    loading_label: RefCell<glib::WeakRef<gtk4::Label>>,
    theme: SharedTheme,
    present: Rc<dyn Fn(Rc<WindowDialog>)>,
    heading: glib::WeakRef<gtk4::Label>,
    go_back: Rc<dyn Fn()>,
    open_ipv4: Rc<dyn Fn()>,
    open_ipv6: Rc<dyn Fn()>,
    open_eap: Rc<dyn Fn()>,
    pub(super) draft: RefCell<Profile>,
    saved_profile: RefCell<Profile>,
    is_new: Cell<bool>,
    are_secrets_read: Cell<bool>,
    are_secrets_edited: Cell<bool>,
    are_saved_secrets_edited: Cell<bool>,
    devices: RefCell<Vec<String>>,
    port_devices: RefCell<Vec<String>>,
    saved_ports: RefCell<Vec<(String, String)>>,
    is_port_sync_pending: Cell<bool>,
    stale_port_interfaces: RefCell<Vec<String>>,
    errors: RefCell<BTreeMap<String, InvalidText>>,
    field_keys: RefCell<Vec<String>>,
    operation: Cell<Option<Operation>>,
    outcome: RefCell<Option<Result<String, String>>>,
    is_rebuild_queued: Cell<bool>,
    owned_items: RefCell<Vec<Box<dyn Any>>>,
    actions: RefCell<Option<Actions>>,
}

pub fn build(context: &Context) -> Rc<Page> {
    show(context, Part::Main)
}

pub(super) fn show(context: &Context, part: Part) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let status = page.section("", "");
    if !check_manager_running(&page, &status) {
        return page;
    }
    let loading_label = text::styled(&tr("Loading…"));
    text::set_color(&loading_label, "colSubtext");
    loading_label.set_xalign(0.0);
    loading_label.set_margin_start(NOTE_START);
    status.append(&loading_label);
    let form = Page::sections();
    page.append(&form);
    let editor = Editor::find_or_create(context);
    editor.part.set(part);
    editor.page.replace(Rc::downgrade(&page));
    editor.form.replace(form);
    editor.status.replace(status.downgrade());
    editor.loading_label.replace(loading_label.downgrade());
    if editor.is_started.get() {
        editor.hide_status();
        editor.retitle();
        editor.errors.borrow_mut().clear();
        editor.rebuild();
    }
    page
}

impl Editor {
    fn find_or_create(context: &Context) -> Rc<Editor> {
        let argument = context.argument.borrow().clone();
        let shared_editor = context
            .shared
            .borrow()
            .as_ref()
            .and_then(|(_, shared)| shared.clone().downcast::<Editor>().ok())
            .filter(|editor| argument.is_none() || editor.argument == argument);
        if let Some(editor) = shared_editor {
            return editor;
        }
        let editor = Editor::new(context, argument.clone());
        context.shared.replace(Some(("connection", editor.clone())));
        glib::spawn_future_local({
            let editor = Rc::downgrade(&editor);
            async move {
                let start_result = start(argument).await;
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                match start_result {
                    Ok(start_state) => {
                        editor.hide_status();
                        editor.open(start_state);
                    }
                    Err(message) => {
                        if let Some(loading_label) = editor.loading_label.borrow().upgrade() {
                            loading_label.set_text(&message);
                            text::set_color(&loading_label, "colError");
                        }
                    }
                }
            }
        });
        editor
    }

    fn new(context: &Context, argument: Option<String>) -> Rc<Editor> {
        Rc::new(Editor {
            argument,
            is_started: Cell::new(false),
            part: Cell::new(Part::Main),
            page: RefCell::new(Weak::new()),
            form: RefCell::new(Page::sections()),
            status: RefCell::new(glib::WeakRef::new()),
            loading_label: RefCell::new(glib::WeakRef::new()),
            theme: context.theme.clone(),
            present: Rc::new(context.dialog_presenter()),
            heading: context.heading.clone(),
            go_back: Rc::new(context.go_back()),
            open_ipv4: Rc::new(context.subpage_opener("ipv4")),
            open_ipv6: Rc::new(context.subpage_opener("ipv6")),
            open_eap: Rc::new(context.subpage_opener("eap")),
            draft: RefCell::default(),
            saved_profile: RefCell::default(),
            is_new: Cell::new(false),
            are_secrets_read: Cell::new(false),
            are_secrets_edited: Cell::new(false),
            are_saved_secrets_edited: Cell::new(false),
            devices: RefCell::default(),
            port_devices: RefCell::default(),
            saved_ports: RefCell::default(),
            is_port_sync_pending: Cell::new(false),
            stale_port_interfaces: RefCell::default(),
            errors: RefCell::default(),
            field_keys: RefCell::default(),
            operation: Cell::new(None),
            outcome: RefCell::new(None),
            is_rebuild_queued: Cell::new(false),
            owned_items: RefCell::default(),
            actions: RefCell::new(None),
        })
    }

    fn hide_status(&self) {
        if let Some(card) = self
            .status
            .borrow()
            .upgrade()
            .and_then(|status| status.parent())
        {
            card.set_visible(false);
        }
    }

    pub(super) fn leave(&self) {
        (self.go_back)();
    }
}

fn pick_status(
    is_saving: bool,
    problem: Option<String>,
    outcome: Option<&Result<String, String>>,
) -> Option<(String, &'static str)> {
    match (is_saving, problem, outcome) {
        (true, _, _) => Some((tr("Saving…"), "colSubtext")),
        (false, _, Some(Err(message))) => Some((message.clone(), "colError")),
        (false, Some(problem), _) => Some((problem, "colError")),
        (false, None, Some(Ok(message))) => Some((message.clone(), "colSubtext")),
        (false, None, None) => None,
    }
}

pub(super) fn edit_eap(
    profile: &mut Profile,
    change: impl FnOnce(&mut nmprofile::enterprise::Eap),
) {
    let mut eap = profile.eap();
    change(&mut eap);
    profile.set_eap(&eap);
}

pub(super) fn inner_methods(method: &str) -> &'static [(&'static str, &'static str)] {
    match method {
        "ttls" => &TTLS_INNER,
        "peap" => &PEAP_INNER,
        _ => &[],
    }
}

pub(super) fn choices<T: Copy + Into<Value>>(options: &[(&str, T)]) -> Vec<(String, Value)> {
    options
        .iter()
        .map(|(name, value)| (tr(name), (*value).into()))
        .collect()
}

impl Editor {
    fn open(self: &Rc<Self>, start_state: Start) {
        self.devices.replace(start_state.devices);
        self.port_devices.replace(
            start_state
                .ports
                .iter()
                .map(|(_, device)| device.clone())
                .collect(),
        );
        self.saved_ports.replace(start_state.ports);
        self.is_new.set(start_state.is_new);
        self.are_secrets_read.set(start_state.are_secrets_read);
        self.saved_profile.replace(start_state.profile.clone());
        self.draft.replace(start_state.profile);
        self.is_started.set(true);
        self.retitle();
        self.rebuild();
    }

    fn retitle(&self) {
        if !matches!(self.part.get(), Part::Main) {
            return;
        }
        if let Some(heading) = self.heading.upgrade() {
            heading.set_text(&self.saved_profile.borrow().id());
        }
    }

    fn queue_rebuild(self: &Rc<Self>) {
        if self.is_rebuild_queued.replace(true) {
            return;
        }
        let editor = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(editor) = editor.upgrade() {
                editor.is_rebuild_queued.set(false);
                editor.rebuild();
            }
        });
    }

    fn rebuild(self: &Rc<Self>) {
        let Some(page) = self.page.borrow().upgrade() else {
            return;
        };
        let adjustment = page.root.vadjustment();
        let offset = adjustment.value();
        let form = self.form.borrow().clone();
        while let Some(child) = form.first_child() {
            form.remove(&child);
        }
        self.owned_items.borrow_mut().clear();
        self.field_keys.borrow_mut().clear();
        self.actions.replace(None);
        match self.part.get() {
            Part::Main => self.main(&page),
            Part::Sub(fill) => fill(self, &page),
        }
        let field_keys = self.field_keys.borrow();
        self.errors
            .borrow_mut()
            .retain(|key, _| field_keys.contains(key));
        drop(field_keys);
        self.actions_section();
        self.refresh();
        if offset > 0.0 {
            glib::idle_add_local_once(move || adjustment.set_value(offset));
        }
    }

    fn main(self: &Rc<Self>, page: &Page) {
        let kind = self.draft.borrow().kind();
        self.general(page);
        match kind.as_str() {
            WIRED => self.wired(page),
            WIRELESS => self.wireless(page),
            WIREGUARD => {
                self.wireguard(page);
                self.peers(page);
            }
            VPN => self.vpn(page),
            VLAN => self.vlan(page),
            BRIDGE | BOND => self.controller(page, &kind),
            _ => {}
        }
        let families: Vec<Family> = [Family::V4, Family::V6]
            .into_iter()
            .filter(|family| self.draft.borrow().settings.contains_key(family.setting()))
            .collect();
        if families.is_empty() {
            return;
        }
        let section = self.section("", "");
        for family in families {
            let (methods, open): (&[(&str, &str)], _) = match family {
                Family::V4 => (&IPV4_METHODS, self.open_ipv4.clone()),
                Family::V6 => (&IPV6_METHODS, self.open_ipv6.clone()),
            };
            let method = self.draft.borrow().ip(family).method;
            let method_name = methods
                .iter()
                .find(|(_, known)| *known == method)
                .map_or(method.clone(), |(name, _)| tr(name));
            page.link_row(&section, "router", family.name(), &method_name, move || {
                open()
            });
        }
    }

    fn edit(&self, change: impl FnOnce(&mut Profile) -> Result<(), String>) -> Result<(), String> {
        self.outcome.replace(None);
        let mut draft = self.draft.borrow().clone();
        change(&mut draft)?;
        self.draft.replace(draft);
        Ok(())
    }

    fn refresh(&self) {
        let actions = self.actions.borrow();
        let Some(actions) = actions.as_ref() else {
            return;
        };
        let draft = self.draft.borrow();
        let is_new = self.is_new.get();
        let has_unsaved_edits = *draft != *self.saved_profile.borrow()
            || self.ports_differ()
            || !self.errors.borrow().is_empty();
        let is_dirty = is_new || has_unsaved_edits || self.is_port_sync_pending.get();
        let problem = self
            .errors
            .borrow()
            .values()
            .next()
            .map(|invalid_text| invalid_text.message.clone())
            .or_else(|| {
                draft.problem(
                    draft
                        .keeps_secret_of(&self.saved_profile.borrow(), self.are_secrets_read.get()),
                )
            });
        let operation = self.operation.get();
        let is_idle = operation.is_none();
        self.form
            .borrow()
            .set_sensitive(operation != Some(Operation::Delete));
        actions
            .save_button
            .set_sensitive(is_dirty && problem.is_none() && is_idle);
        actions
            .revert_button
            .set_sensitive(has_unsaved_edits && !is_new && is_idle);
        if let Some(delete_button) = &actions.delete_button {
            delete_button.set_sensitive(is_idle);
        }
        let is_saving = operation == Some(Operation::Save);
        let outcome = self.outcome.borrow();
        match pick_status(is_saving, problem.filter(|_| is_dirty), outcome.as_ref()) {
            Some((message, color)) => show_note(&actions.status, Some(&message), color),
            None => show_note(&actions.status, None, "colSubtext"),
        }
    }

    pub(super) fn hold(&self, item: impl Any) {
        self.owned_items.borrow_mut().push(Box::new(item));
    }

    pub(super) fn section(&self, icon: &str, title: &str) -> gtk4::Box {
        Page::section_into(&self.form.borrow(), icon, title)
    }

    pub(super) fn subsection(
        &self,
        page: &Page,
        parent: &gtk4::Box,
        title: &str,
        tip: &str,
    ) -> gtk4::Box {
        let (content, tip) = page.unkept_subsection(parent, title, tip);
        if let Some(tip) = tip {
            self.hold(tip);
        }
        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_shows_ahead_of_a_draft_problem() {
        let problem = || Some("A VLAN needs a parent device".to_owned());
        let failure = Err("Permission denied".to_owned());
        let success = Ok("Saved".to_owned());
        assert_eq!(
            pick_status(false, problem(), Some(&failure)),
            Some(("Permission denied".to_owned(), "colError"))
        );
        assert_eq!(
            pick_status(false, problem(), Some(&success)),
            Some(("A VLAN needs a parent device".to_owned(), "colError"))
        );
        assert_eq!(
            pick_status(false, None, Some(&success)),
            Some(("Saved".to_owned(), "colSubtext"))
        );
        assert_eq!(
            pick_status(false, None, Some(&failure)),
            Some(("Permission denied".to_owned(), "colError"))
        );
        assert_eq!(pick_status(false, None, None), None);
        assert_eq!(
            pick_status(true, problem(), Some(&failure)),
            Some((tr("Saving…"), "colSubtext"))
        );
    }
}
