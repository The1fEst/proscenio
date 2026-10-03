use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::core::{process, tools};
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::panels::settings::pages::network::manager_running;
use crate::platform::nmprofile::{
    self, BOND, BRIDGE, Family, Peer, Profile, Security, VLAN, VPN, WIRED, WIREGUARD, WIRELESS,
};
use crate::services::net::nmcli;
use crate::services::nmsettings;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::controls::{ComboBox, ConfigSwitch, icon_button};
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::spinbox::SpinBox;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

pub const NEW_WIRED: &str = "new:802-3-ethernet";
pub const NEW_WIREGUARD: &str = "new:wireguard";
pub const NEW_ENTERPRISE: &str = "new:enterprise:";
pub const NEW_OPENVPN: &str = "new:openvpn";
pub const NEW_VLAN: &str = "new:vlan";
pub const NEW_BRIDGE: &str = "new:bridge";
pub const NEW_BOND: &str = "new:bond";
const VLAN_ID_MAX: i64 = 4094;
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
const KEEPALIVE_MAX: i64 = 3600;
const DIALOG_WIDTH: f64 = 400.0;

const METERED: [(&str, i32); 3] = [("Automatic", 0), ("Yes", 1), ("No", 2)];
const SECURITY: [(&str, Security); 5] = [
    ("None", Security::Open),
    ("WPA & WPA2 Personal", Security::Personal),
    ("WPA3 Personal", Security::Wpa3),
    ("WPA & WPA2 Enterprise", Security::Enterprise),
    ("WEP", Security::Wep),
];
pub(super) const EAP_METHODS: [(&str, &str); 3] = [
    ("Protected EAP (PEAP)", "peap"),
    ("Tunneled TLS (TTLS)", "ttls"),
    ("TLS", "tls"),
];
const PEAP_INNER: [(&str, &str); 3] = [("MSCHAPv2", "mschapv2"), ("GTC", "gtc"), ("MD5", "md5")];
const OPENVPN_KINDS: [(&str, &str); 4] = [
    ("Certificates", "tls"),
    ("Password", "password"),
    ("Password and certificates", "password-tls"),
    ("Static key", "static-key"),
];
const KEY_DIRECTIONS: [(&str, &str); 3] = [("No direction", ""), ("0", "0"), ("1", "1")];
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

type PeerEdit = Box<dyn FnOnce(&mut Peer) -> Result<(), String>>;

struct Start {
    profile: Profile,
    fresh: bool,
    secrets: bool,
    devices: Vec<String>,
    ports: Vec<(String, String)>,
}

struct Actions {
    save: RippleButton,
    revert: RippleButton,
    status: gtk4::Label,
}

#[derive(Clone, Copy)]
pub(super) enum Part {
    Main,
    Sub(fn(&Rc<Editor>, &Page)),
}

pub(super) struct Editor {
    argument: Option<String>,
    started: Cell<bool>,
    part: Cell<Part>,
    page: RefCell<Weak<Page>>,
    form: RefCell<gtk4::Box>,
    status: RefCell<glib::WeakRef<gtk4::Box>>,
    loading: RefCell<glib::WeakRef<gtk4::Label>>,
    theme: SharedTheme,
    present: Rc<dyn Fn(Rc<WindowDialog>)>,
    heading: glib::WeakRef<gtk4::Label>,
    back: Rc<dyn Fn()>,
    open_ipv4: Rc<dyn Fn()>,
    open_ipv6: Rc<dyn Fn()>,
    open_eap: Rc<dyn Fn()>,
    pub(super) draft: RefCell<Profile>,
    saved: RefCell<Profile>,
    fresh: Cell<bool>,
    secrets: Cell<bool>,
    secrets_edited: Cell<bool>,
    devices: RefCell<Vec<String>>,
    ports: RefCell<Vec<String>>,
    saved_ports: RefCell<Vec<(String, String)>>,
    errors: RefCell<BTreeMap<String, String>>,
    busy: Cell<bool>,
    outcome: RefCell<Option<Result<String, String>>>,
    rebuild_queued: Cell<bool>,
    held: RefCell<Vec<Box<dyn Any>>>,
    actions: RefCell<Option<Actions>>,
}

pub fn build(context: &Context) -> Rc<Page> {
    show(context, Part::Main)
}

pub(super) fn show(context: &Context, part: Part) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let status = page.section("", "");
    if !manager_running(&page, &status) {
        return page;
    }
    let loading = text::styled(&tr("Loading…"));
    text::set_color(&loading, "colSubtext");
    loading.set_xalign(0.0);
    loading.set_margin_start(NOTE_START);
    status.append(&loading);
    let form = Page::sections();
    page.append(&form);
    let editor = Editor::shared(context);
    editor.part.set(part);
    editor.page.replace(Rc::downgrade(&page));
    editor.form.replace(form);
    editor.status.replace(status.downgrade());
    editor.loading.replace(loading.downgrade());
    if editor.started.get() {
        editor.hide_status();
        editor.retitle();
        editor.rebuild();
    }
    page
}

impl Editor {
    fn shared(context: &Context) -> Rc<Editor> {
        let argument = context.argument.borrow().clone();
        let kept = context
            .shared
            .borrow()
            .as_ref()
            .and_then(|(_, kept)| kept.clone().downcast::<Editor>().ok())
            .filter(|editor| argument.is_none() || editor.argument == argument);
        if let Some(editor) = kept {
            return editor;
        }
        let editor = Editor::new(context, argument.clone());
        context.shared.replace(Some(("connection", editor.clone())));
        glib::spawn_future_local({
            let editor = Rc::downgrade(&editor);
            async move {
                let started = start(argument).await;
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                match started {
                    Ok(started) => {
                        editor.hide_status();
                        editor.open(started);
                    }
                    Err(message) => {
                        if let Some(loading) = editor.loading.borrow().upgrade() {
                            loading.set_text(&message);
                            text::set_color(&loading, "colError");
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
            started: Cell::new(false),
            part: Cell::new(Part::Main),
            page: RefCell::new(Weak::new()),
            form: RefCell::new(Page::sections()),
            status: RefCell::new(glib::WeakRef::new()),
            loading: RefCell::new(glib::WeakRef::new()),
            theme: context.theme.clone(),
            present: Rc::new(context.dialog_presenter()),
            heading: context.heading.clone(),
            back: Rc::new(context.go_back()),
            open_ipv4: Rc::new(context.subpage_opener("ipv4")),
            open_ipv6: Rc::new(context.subpage_opener("ipv6")),
            open_eap: Rc::new(context.subpage_opener("eap")),
            draft: RefCell::default(),
            saved: RefCell::default(),
            fresh: Cell::new(false),
            secrets: Cell::new(false),
            secrets_edited: Cell::new(false),
            devices: RefCell::default(),
            ports: RefCell::default(),
            saved_ports: RefCell::default(),
            errors: RefCell::default(),
            busy: Cell::new(false),
            outcome: RefCell::new(None),
            rebuild_queued: Cell::new(false),
            held: RefCell::default(),
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
        (self.back)();
    }
}

async fn primary() -> Option<String> {
    nmcli(&["-t", "-f", "UUID,TYPE", "connection", "show", "--active"])
        .await
        .output
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(_, kind)| *kind != "loopback")
        .map(|(uuid, _)| uuid.to_owned())
}

async fn start(argument: Option<String>) -> Result<Start, String> {
    let wanted = match argument {
        Some(wanted) => wanted,
        None => primary()
            .await
            .ok_or_else(|| tr("No connection is active, so there is nothing to edit"))?,
    };
    if let Some(ssid) = wanted.strip_prefix(NEW_ENTERPRISE) {
        let (names, _) = nmsettings::names_and_interfaces().await;
        let mut profile = Profile::new(WIRELESS, &nmprofile::free_name(ssid, &names));
        profile.set_ssid(ssid);
        profile.set_security(Security::Enterprise);
        return Ok(Start {
            devices: Vec::new(),
            profile,
            fresh: true,
            secrets: true,
            ports: Vec::new(),
        });
    }
    if wanted == NEW_OPENVPN {
        let (names, _) = nmsettings::names_and_interfaces().await;
        return Ok(Start {
            devices: Vec::new(),
            profile: Profile::new_openvpn(&nmprofile::free_name(&tr("OpenVPN"), &names)),
            fresh: true,
            secrets: true,
            ports: Vec::new(),
        });
    }
    if let Some(kind) = wanted.strip_prefix(NEW) {
        let base = match kind {
            WIREGUARD => tr("WireGuard"),
            VLAN => tr("VLAN"),
            BRIDGE => tr("Bridge"),
            BOND => tr("Bond"),
            _ => tr("Wired connection"),
        };
        let (names, interfaces) = nmsettings::names_and_interfaces().await;
        let mut profile = Profile::new(kind, &nmprofile::free_name(&base, &names));
        match kind {
            WIREGUARD => {
                profile.set_interface(&nmprofile::free_interface("wg", &interfaces));
                if let Some(key) = nmsettings::generate_key().await {
                    profile.set_private_key(&key);
                }
            }
            BRIDGE => {
                profile.set_interface(&nmprofile::free_interface("br", &interfaces));
                profile.set_stp(true);
            }
            BOND => {
                profile.set_interface(&nmprofile::free_interface("bond", &interfaces));
                profile.set_bond_mode(BOND_MODES[0].1);
            }
            _ => {}
        }
        return Ok(Start {
            devices: nmsettings::devices("ethernet").await,
            profile,
            fresh: true,
            secrets: true,
            ports: Vec::new(),
        });
    }
    let loaded = nmsettings::load(&wanted).await?;
    let profile = loaded.profile.normalized();
    let ports = if matches!(profile.kind().as_str(), BRIDGE | BOND) {
        nmsettings::ports(&[profile.interface(), profile.uuid()]).await
    } else {
        Vec::new()
    };
    Ok(Start {
        devices: nmsettings::devices("ethernet").await,
        profile,
        fresh: false,
        secrets: loaded.secrets,
        ports,
    })
}

fn note() -> gtk4::Label {
    let label = text::styled_sized("", pixel_size::SMALLER);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_margin_start(NOTE_START);
    label.set_visible(false);
    label
}

fn show_note(label: &gtk4::Label, message: Option<&str>, color: &str) {
    label.set_visible(message.is_some());
    label.set_text(message.unwrap_or_default());
    text::set_color(label, color);
}

async fn sync_ports(
    controller: &Profile,
    previous: &str,
    wanted: &[String],
    had: &[(String, String)],
) -> Result<(), String> {
    let interface = controller.interface();
    let renamed = !previous.is_empty() && previous != interface;
    let automatic = controller.autoconnect();
    for (uuid, device) in had {
        if renamed || !wanted.contains(device) {
            nmsettings::delete(uuid).await?;
        } else {
            nmsettings::set_autoconnect(uuid, automatic).await?;
        }
    }
    for device in wanted {
        let kept = !renamed && had.iter().any(|(_, known)| known == device);
        if !kept {
            let mut port = Profile::new_port(&interface, &controller.kind(), device);
            port.set_autoconnect(automatic);
            nmsettings::add(&port).await?;
        }
    }
    Ok(())
}

pub(super) fn edit_eap(profile: &mut Profile, change: impl FnOnce(&mut nmprofile::Eap)) {
    let mut eap = profile.eap();
    change(&mut eap);
    profile.set_eap(&eap);
}

fn edit_openvpn(profile: &mut Profile, change: impl FnOnce(&mut nmprofile::OpenVpn)) {
    let mut vpn = profile.openvpn();
    change(&mut vpn);
    profile.set_openvpn(&vpn);
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
    fn open(self: &Rc<Self>, started: Start) {
        self.devices.replace(started.devices);
        self.ports.replace(
            started
                .ports
                .iter()
                .map(|(_, device)| device.clone())
                .collect(),
        );
        self.saved_ports.replace(started.ports);
        self.fresh.set(started.fresh);
        self.secrets.set(started.secrets);
        self.saved.replace(started.profile.clone());
        self.draft.replace(started.profile);
        self.started.set(true);
        self.retitle();
        self.rebuild();
    }

    fn retitle(&self) {
        if !matches!(self.part.get(), Part::Main) {
            return;
        }
        if let Some(heading) = self.heading.upgrade() {
            heading.set_text(&self.saved.borrow().id());
        }
    }

    fn queue_rebuild(self: &Rc<Self>) {
        if self.rebuild_queued.replace(true) {
            return;
        }
        let editor = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(editor) = editor.upgrade() {
                editor.rebuild_queued.set(false);
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
        self.held.borrow_mut().clear();
        self.errors.borrow_mut().clear();
        self.actions.replace(None);
        match self.part.get() {
            Part::Main => self.main(&page),
            Part::Sub(fill) => fill(self, &page),
        }
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
            let shown = methods
                .iter()
                .find(|(_, known)| *known == method)
                .map_or(method.clone(), |(name, _)| tr(name));
            page.link_row(&section, "router", family.name(), &shown, move || open());
        }
    }

    fn eap_link(self: &Rc<Self>, page: &Page, parent: &gtk4::Box) {
        let method = self.draft.borrow().eap().method;
        let shown = EAP_METHODS
            .iter()
            .find(|(_, known)| *known == method)
            .map_or(method.clone(), |(name, _)| tr(name));
        let open = self.open_eap.clone();
        page.link_row(
            parent,
            "shield_lock",
            &tr("Authentication"),
            &shown,
            move || open(),
        );
    }

    fn edit(&self, change: impl FnOnce(&mut Profile) -> Result<(), String>) -> Result<(), String> {
        let mut draft = self.draft.borrow().clone();
        change(&mut draft)?;
        self.draft.replace(draft);
        self.outcome.replace(None);
        Ok(())
    }

    fn refresh(&self) {
        let actions = self.actions.borrow();
        let Some(actions) = actions.as_ref() else {
            return;
        };
        let draft = self.draft.borrow();
        let fresh = self.fresh.get();
        let dirty = fresh || *draft != *self.saved.borrow() || self.ports_changed();
        let problem = self
            .errors
            .borrow()
            .values()
            .next()
            .cloned()
            .or_else(|| draft.problem());
        let busy = self.busy.get();
        actions
            .save
            .set_sensitive(dirty && problem.is_none() && !busy);
        actions.revert.set_sensitive(dirty && !fresh && !busy);
        let outcome = self.outcome.borrow();
        match (busy, problem.filter(|_| dirty), outcome.as_ref()) {
            (true, _, _) => show_note(&actions.status, Some(&tr("Saving…")), "colSubtext"),
            (false, Some(problem), _) => show_note(&actions.status, Some(&problem), "colError"),
            (false, None, Some(Ok(message))) => {
                show_note(&actions.status, Some(message), "colSubtext")
            }
            (false, None, Some(Err(message))) => {
                show_note(&actions.status, Some(message), "colError")
            }
            (false, None, None) => show_note(&actions.status, None, "colSubtext"),
        }
    }

    pub(super) fn hold(&self, held: impl Any) {
        self.held.borrow_mut().push(Box::new(held));
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

    pub(super) fn field(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        label: &str,
        value: &str,
        apply: impl Fn(&str, &mut Profile) -> Result<(), String> + 'static,
    ) -> Rc<TextField> {
        let field = TextField::new(&self.theme, Style::Outlined, label);
        self.watch(parent, field.clone(), value, false, apply);
        field
    }

    pub(super) fn secret_field(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        label: &str,
        value: &str,
        apply: impl Fn(&str, &mut Profile) -> Result<(), String> + 'static,
    ) -> Rc<TextField> {
        let field = TextField::secret(&self.theme, Style::Outlined, label);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
        let shown = Cell::new(false);
        let (reveal, _) = icon_button(&self.theme, "visibility", false, "");
        reveal.connect_clicked({
            let editor = Rc::downgrade(self);
            let field = Rc::downgrade(&field);
            move |_| {
                let (Some(editor), Some(field)) = (editor.upgrade(), field.upgrade()) else {
                    return;
                };
                if editor.secrets.get() {
                    shown.set(!shown.get());
                    field.set_text_visible(shown.get());
                } else {
                    editor.reveal_secrets();
                }
            }
        });
        let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        holder.set_hexpand(true);
        row.append(&holder);
        row.append(&reveal);
        parent.append(&row);
        self.watch(&holder, field.clone(), value, true, apply);
        if !self.secrets.get() && value.is_empty() {
            let hint = note();
            show_note(
                &hint,
                Some(&tr(
                    "Stored and hidden. Type to replace it, or press the eye to show it",
                )),
                "colSubtext",
            );
            parent.append(&hint);
        }
        field
    }

    fn watch(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        field: Rc<TextField>,
        value: &str,
        secret: bool,
        apply: impl Fn(&str, &mut Profile) -> Result<(), String> + 'static,
    ) {
        field.root.set_hexpand(true);
        field.set_text(value);
        parent.append(&field.root);
        let problem = note();
        parent.append(&problem);
        let key = format!("{:p}", Rc::as_ptr(&field));
        field.connect_changed({
            let editor = Rc::downgrade(self);
            let field = Rc::downgrade(&field);
            move || {
                let (Some(editor), Some(field)) = (editor.upgrade(), field.upgrade()) else {
                    return;
                };
                let text = field.text();
                match editor.edit(|profile| apply(&text, profile)) {
                    Ok(()) => {
                        editor.errors.borrow_mut().remove(&key);
                        show_note(&problem, None, "colError");
                    }
                    Err(message) => {
                        show_note(&problem, Some(&message), "colError");
                        editor.errors.borrow_mut().insert(key.clone(), message);
                    }
                }
                if secret {
                    editor.secrets_edited.set(true);
                }
                editor.refresh();
            }
        });
        self.hold(field);
    }

    pub(super) fn switch(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        icon: &str,
        label: &str,
        read: impl Fn(&Profile) -> bool + 'static,
        write: impl Fn(&mut Profile, bool) + 'static,
    ) -> Rc<ConfigSwitch> {
        let switch = ConfigSwitch::new(&self.theme, icon, label, {
            let editor = Rc::downgrade(self);
            move |wanted| {
                if let Some(editor) = editor.upgrade() {
                    let _ = editor.edit(|profile| {
                        write(profile, wanted);
                        Ok(())
                    });
                    editor.queue_rebuild();
                }
            }
        });
        switch.button.set_hexpand(true);
        switch.bind({
            let editor = Rc::downgrade(self);
            move || {
                editor
                    .upgrade()
                    .is_some_and(|editor| read(&editor.draft.borrow()))
            }
        });
        parent.append(&switch.button);
        self.hold(switch.clone());
        switch
    }

    pub(super) fn choose(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        options: Vec<(String, Value)>,
        current: Value,
        write: impl Fn(&mut Profile, &Value) + 'static,
    ) {
        let choices = options
            .into_iter()
            .map(|(label, value)| Choice {
                label,
                icon: "",
                value,
            })
            .collect();
        let selection = Selection::new(&self.theme, choices, {
            let editor = Rc::downgrade(self);
            move |value| {
                if let Some(editor) = editor.upgrade() {
                    let _ = editor.edit(|profile| {
                        write(profile, &value);
                        Ok(())
                    });
                    editor.queue_rebuild();
                }
            }
        });
        selection.set_current(&current);
        parent.append(&selection.root);
        self.hold(selection);
    }

    fn spin(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        icon: &str,
        label: &str,
        range: (i64, i64),
        value: i64,
        write: impl Fn(&mut Profile, i64) + 'static,
    ) {
        let spin = SpinBox::new(&self.theme, range.0, range.1, 1, 0);
        spin.set_value(value);
        spin.connect_changed({
            let editor = Rc::downgrade(self);
            move |value| {
                if let Some(editor) = editor.upgrade() {
                    let _ = editor.edit(|profile| {
                        write(profile, value);
                        Ok(())
                    });
                    editor.refresh();
                }
            }
        });
        Page::unkept_spin_row(parent, icon, label, &spin);
        self.hold(spin);
    }

    fn general(self: &Rc<Self>, page: &Page) {
        let section = self.section("tune", &tr("General"));
        let profile = self.draft.borrow().clone();
        self.field(&section, &tr("Name"), &profile.id(), |text, profile| {
            profile.set_id(text.trim());
            Ok(())
        });
        self.switch(
            &section,
            "autorenew",
            &tr("Connect automatically"),
            Profile::autoconnect,
            Profile::set_autoconnect,
        );
        let everyone = self.switch(
            &section,
            "group",
            &tr("Available to all users"),
            Profile::all_users,
            |profile, all| profile.set_all_users(all, &glib::user_name().to_string_lossy()),
        );
        self.hold(page.unkept_tip(
            &everyone.button,
            &tr("Off keeps the connection to your own account"),
        ));
        let metered = self.subsection(
            page,
            &section,
            &tr("Metered connection"),
            &tr("A connection paid for by the amount of data. Apps hold back on large downloads over it"),
        );
        let current = match profile.metered() {
            1 | 3 => 1,
            2 | 4 => 2,
            _ => 0,
        };
        self.choose(
            &metered,
            choices(&METERED),
            Value::from(current),
            |profile, value| profile.set_metered(value.as_i64().unwrap_or(0) as i32),
        );
    }

    fn mac_and_mtu(self: &Rc<Self>, parent: &gtk4::Box, profile: &Profile) {
        self.field(
            parent,
            &tr("Cloned MAC address"),
            &profile.cloned_mac(),
            |text, profile| {
                profile.set_cloned_mac(&nmprofile::parse_mac(text)?);
                Ok(())
            },
        );
        self.spin(
            parent,
            "straighten",
            &tr("MTU (0 picks it automatically)"),
            (0, MTU_MAX),
            profile.mtu() as i64,
            |profile, mtu| profile.set_mtu(mtu as u32),
        );
    }

    fn device_combo(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        current: &str,
        any: Option<String>,
        write: impl Fn(&mut Profile, &str) + 'static,
    ) {
        let mut items: Vec<String> = any.iter().cloned().collect();
        items.extend(self.devices.borrow().iter().cloned());
        if !current.is_empty() && !items.iter().any(|item| item == current) {
            items.push(current.to_owned());
        }
        let index = items.iter().position(|item| item == current).unwrap_or(0);
        let combo = ComboBox::new(&self.theme);
        combo.set_icon("settings_ethernet");
        combo.button.set_hexpand(true);
        combo.set_items(&items, index as i32);
        let first_is_any = any.is_some();
        combo.connect_activated({
            let editor = Rc::downgrade(self);
            let combo = Rc::downgrade(&combo);
            move |index| {
                let (Some(editor), Some(combo)) = (editor.upgrade(), combo.upgrade()) else {
                    return;
                };
                let name = if first_is_any && index == 0 {
                    String::new()
                } else {
                    items.get(index).cloned().unwrap_or_default()
                };
                let _ = editor.edit(|profile| {
                    write(profile, &name);
                    Ok(())
                });
                combo.set_items(&items, index as i32);
                editor.refresh();
            }
        });
        parent.append(&combo.button);
        self.hold(combo);
    }

    fn ports_changed(&self) -> bool {
        let mut saved: Vec<String> = self
            .saved_ports
            .borrow()
            .iter()
            .map(|(_, device)| device.clone())
            .collect();
        let mut wanted = self.ports.borrow().clone();
        saved.sort();
        wanted.sort();
        saved != wanted
    }

    fn vlan(self: &Rc<Self>, page: &Page) {
        let section = self.section("lan", &tr("VLAN"));
        let profile = self.draft.borrow().clone();
        let parent = self.subsection(page, &section, &tr("Parent device"), "");
        self.device_combo(
            &parent,
            &profile.vlan_parent(),
            None,
            Profile::set_vlan_parent,
        );
        if profile.vlan_parent().is_empty()
            && let Some(first) = self.devices.borrow().first()
        {
            let first = first.clone();
            let _ = self.edit(|profile| {
                profile.set_vlan_parent(&first);
                Ok(())
            });
        }
        self.spin(
            &section,
            "tag",
            &tr("VLAN ID"),
            (1, VLAN_ID_MAX),
            profile.vlan_id().max(1) as i64,
            |profile, id| profile.set_vlan_id(id as u32),
        );
        if profile.vlan_id() == 0 {
            let _ = self.edit(|profile| {
                profile.set_vlan_id(1);
                Ok(())
            });
        }
        self.field(
            &section,
            &tr("Interface name (empty picks one)"),
            &profile.interface(),
            |text, profile| {
                profile.set_interface(text.trim());
                Ok(())
            },
        );
    }

    fn controller(self: &Rc<Self>, page: &Page, kind: &str) {
        let bridge = kind == BRIDGE;
        let section = if bridge {
            self.section("device_hub", &tr("Bridge"))
        } else {
            self.section("join", &tr("Bond"))
        };
        let profile = self.draft.borrow().clone();
        self.field(
            &section,
            &tr("Interface name"),
            &profile.interface(),
            |text, profile| {
                profile.set_interface(text.trim());
                Ok(())
            },
        );
        if bridge {
            let stp = self.switch(
                &section,
                "account_tree",
                &tr("Spanning tree (STP)"),
                Profile::stp,
                Profile::set_stp,
            );
            self.hold(page.unkept_tip(
                &stp.button,
                &tr("Keeps loops out when the bridge connects to other switches"),
            ));
        } else {
            let mode = self.subsection(page, &section, &tr("Mode"), "");
            self.choose(
                &mode,
                choices(&BOND_MODES),
                Value::from(profile.bond_mode()),
                |profile, value| profile.set_bond_mode(value.as_str().unwrap_or("active-backup")),
            );
        }
        let ports = self.subsection(
            page,
            &section,
            &tr("Ports"),
            &tr("The devices that join it. A device that joins leaves its own connection"),
        );
        if self.devices.borrow().is_empty() {
            let empty = note();
            show_note(&empty, Some(&tr("No wired devices")), "colSubtext");
            ports.append(&empty);
        }
        let devices = self.devices.borrow().clone();
        for device in devices {
            let switch = ConfigSwitch::new(&self.theme, "settings_ethernet", &device, {
                let editor = Rc::downgrade(self);
                let device = device.clone();
                move |wanted| {
                    let Some(editor) = editor.upgrade() else {
                        return;
                    };
                    {
                        let mut ports = editor.ports.borrow_mut();
                        ports.retain(|known| *known != device);
                        if wanted {
                            ports.push(device.clone());
                        }
                    }
                    editor.outcome.replace(None);
                    editor.refresh();
                }
            });
            switch.button.set_hexpand(true);
            switch.bind({
                let editor = Rc::downgrade(self);
                move || {
                    editor
                        .upgrade()
                        .is_some_and(|editor| editor.ports.borrow().contains(&device))
                }
            });
            ports.append(&switch.button);
            self.hold(switch);
        }
    }

    fn wired(self: &Rc<Self>, page: &Page) {
        let section = self.section("lan", &tr("Wired"));
        let profile = self.draft.borrow().clone();
        self.device_combo(
            &section,
            &profile.interface(),
            Some(tr("Any device")),
            Profile::set_interface,
        );
        self.mac_and_mtu(&section, &profile);
        let security = self.switch(
            &section,
            "shield_lock",
            &tr("802.1X security"),
            Profile::has_eap,
            Profile::set_wired_eap,
        );
        self.hold(page.unkept_tip(
            &security.button,
            &tr("Signs in to the network port, as office and campus networks ask"),
        ));
        if profile.has_eap() {
            self.eap_link(page, &section);
        }
    }

    fn wireless(self: &Rc<Self>, page: &Page) {
        let section = self.section("wifi", &tr("Wi-Fi"));
        let profile = self.draft.borrow().clone();
        self.field(
            &section,
            &tr("Network name"),
            &profile.ssid(),
            |text, profile| {
                profile.set_ssid(text);
                Ok(())
            },
        );
        self.switch(
            &section,
            "visibility_off",
            &tr("Hidden network"),
            Profile::hidden,
            Profile::set_hidden,
        );
        let security = self.subsection(page, &section, &tr("Security"), "");
        match profile.security() {
            Security::Other => {
                page.notice(
                    &security,
                    "info",
                    &tr("This network uses a kind of security the editor does not offer, which stays as it is"),
                );
            }
            current => {
                let options: Vec<(String, Value)> = SECURITY
                    .iter()
                    .enumerate()
                    .map(|(index, (name, _))| (tr(name), Value::from(index)))
                    .collect();
                let index = SECURITY
                    .iter()
                    .position(|(_, security)| *security == current)
                    .unwrap_or(0);
                self.choose(&security, options, Value::from(index), |profile, value| {
                    let index = value.as_u64().unwrap_or(0) as usize;
                    profile.set_security(SECURITY[index].1);
                });
                match current {
                    Security::Personal | Security::Wpa3 => {
                        self.secret_field(
                            &security,
                            &tr("Password"),
                            &profile.psk(),
                            |text, profile| {
                                profile.set_psk(text);
                                Ok(())
                            },
                        );
                    }
                    Security::Wep => {
                        self.secret_field(
                            &security,
                            &tr("Key"),
                            &profile.wep_key(),
                            |text, profile| {
                                profile.set_wep_key(text);
                                Ok(())
                            },
                        );
                    }
                    Security::Enterprise => self.eap_link(page, &security),
                    Security::Open | Security::Other => {}
                }
            }
        }
        self.mac_and_mtu(&section, &profile);
    }

    pub(super) fn certificate_field(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        label: &str,
        value: &str,
        apply: impl Fn(&mut Profile, String) + 'static,
    ) {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
        let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        holder.set_hexpand(true);
        row.append(&holder);
        let (pick, _) = icon_button(&self.theme, "folder_open", false, "");
        row.append(&pick);
        parent.append(&row);
        let field = TextField::new(&self.theme, Style::Outlined, label);
        self.watch(
            &holder,
            field.clone(),
            value,
            false,
            move |text, profile| {
                apply(profile, text.trim().to_owned());
                Ok(())
            },
        );
        if !tools::missing(&[&tools::KDIALOG]).is_empty() {
            pick.set_sensitive(false);
            return;
        }
        let title = label.to_owned();
        let field = Rc::downgrade(&field);
        pick.connect_clicked(move |_| {
            let field = field.clone();
            let title = title.clone();
            glib::spawn_future_local(async move {
                let home = glib::home_dir().to_string_lossy().into_owned();
                let picker = process::command(&[
                    "kdialog",
                    "--getopenfilename",
                    &home,
                    &format!(
                        "*.pem *.crt *.cer *.der *.key *.p12 *.pfx|{}",
                        tr("Certificates and keys")
                    ),
                    "--title",
                    &title,
                ]);
                let chosen = process::capture_text(picker)
                    .await
                    .map(|path| path.trim().to_owned())
                    .filter(|path| !path.is_empty());
                if let (Some(path), Some(field)) = (chosen, field.upgrade()) {
                    field.set_text(&path);
                }
            });
        });
    }

    fn wireguard(self: &Rc<Self>, page: &Page) {
        let section = self.section("vpn_key", &tr("WireGuard"));
        let profile = self.draft.borrow().clone();
        self.field(
            &section,
            &tr("Interface name"),
            &profile.interface(),
            |text, profile| {
                profile.set_interface(text.trim());
                Ok(())
            },
        );
        let keys = self.subsection(page, &section, &tr("Keys"), "");
        let public = text::styled_sized("", pixel_size::SMALLER);
        text::set_color(&public, "colSubtext");
        public.set_xalign(0.0);
        public.set_selectable(true);
        public.set_wrap(true);
        public.set_wrap_mode(gtk4::pango::WrapMode::Char);
        public.set_margin_start(NOTE_START);
        let show_public = {
            let public = public.downgrade();
            move |key: String| {
                let Some(public) = public.upgrade() else {
                    return;
                };
                public.set_visible(nmprofile::valid_key(&key));
                if !public.is_visible() {
                    return;
                }
                glib::spawn_future_local(async move {
                    let derived = nmsettings::public_key(&key).await.unwrap_or_default();
                    public.set_text(&trf("Public key: %1", &[&derived]));
                });
            }
        };
        show_public(profile.private_key());
        let private = self.secret_field(
            &keys,
            &tr("Private key"),
            &profile.private_key(),
            move |text, profile| {
                profile.set_private_key(text);
                show_public(text.trim().to_owned());
                Ok(())
            },
        );
        keys.append(&public);
        let (generate, _) = icon_button(&self.theme, "key", true, &tr("Generate a new key"));
        generate.set_halign(gtk4::Align::Start);
        generate.connect_clicked({
            let private = Rc::downgrade(&private);
            move |_| {
                let private = private.clone();
                glib::spawn_future_local(async move {
                    let key = nmsettings::generate_key().await;
                    if let (Some(private), Some(key)) = (private.upgrade(), key) {
                        private.set_text(&key);
                    }
                });
            }
        });
        let missing = tools::missing(&[&tools::WG]);
        if !missing.is_empty() {
            generate.set_sensitive(false);
            self.hold(page.unkept_tip(
                &generate,
                &tools::missing_message(&missing, &tr("no key is made")),
            ));
        }
        keys.append(&generate);
        self.spin(
            &section,
            "settings_input_antenna",
            &tr("Listen port (0 picks one)"),
            (0, PORT_MAX),
            profile.listen_port() as i64,
            |profile, port| profile.set_listen_port(port as u32),
        );
        self.spin(
            &section,
            "straighten",
            &tr("MTU (0 picks it automatically)"),
            (0, MTU_MAX),
            profile.mtu() as i64,
            |profile, mtu| profile.set_mtu(mtu as u32),
        );
    }

    fn peers(self: &Rc<Self>, page: &Page) {
        let section = self.section("hub", &tr("Peers"));
        let peers = self.draft.borrow().peers();
        if peers.is_empty() {
            let empty = note();
            show_note(&empty, Some(&tr("No peers yet")), "colSubtext");
            section.append(&empty);
        }
        for (index, peer) in peers.iter().enumerate() {
            let group = self.subsection(
                page,
                &section,
                &trf("Peer %1", &[&(index + 1).to_string()]),
                "",
            );
            let change = move |edit: PeerEdit, profile: &mut Profile| -> Result<(), String> {
                let mut peers = profile.peers();
                if let Some(peer) = peers.get_mut(index) {
                    edit(peer)?;
                }
                profile.set_peers(&peers);
                Ok(())
            };
            self.field(
                &group,
                &tr("Public key"),
                &peer.public_key,
                move |text, profile| {
                    let text = text.trim().to_owned();
                    change(
                        Box::new(move |peer| {
                            peer.public_key = text;
                            Ok(())
                        }),
                        profile,
                    )
                },
            );
            self.field(
                &group,
                &tr("Endpoint"),
                &peer.endpoint,
                move |text, profile| {
                    let endpoint = nmprofile::parse_endpoint(text)?;
                    change(
                        Box::new(move |peer| {
                            peer.endpoint = endpoint;
                            Ok(())
                        }),
                        profile,
                    )
                },
            );
            self.field(
                &group,
                &tr("Allowed IPs"),
                &peer.allowed_ips.join(", "),
                move |text, profile| {
                    let networks = nmprofile::parse_networks(text)?;
                    change(
                        Box::new(move |peer| {
                            peer.allowed_ips = networks;
                            Ok(())
                        }),
                        profile,
                    )
                },
            );
            self.secret_field(
                &group,
                &tr("Preshared key (optional)"),
                &peer.preshared_key,
                move |text, profile| {
                    let key = text.trim().to_owned();
                    change(
                        Box::new(move |peer| {
                            peer.preshared_key = key;
                            Ok(())
                        }),
                        profile,
                    )
                },
            );
            self.spin(
                &group,
                "timer",
                &tr("Keepalive in seconds (0 is off)"),
                (0, KEEPALIVE_MAX),
                peer.keepalive as i64,
                move |profile, seconds| {
                    let _ = change(
                        Box::new(move |peer| {
                            peer.keepalive = seconds as u32;
                            Ok(())
                        }),
                        profile,
                    );
                },
            );
            let (remove, _) = icon_button(&self.theme, "delete", false, &tr("Remove peer"));
            remove.set_halign(gtk4::Align::Start);
            remove.connect_clicked({
                let editor = Rc::downgrade(self);
                move |_| {
                    if let Some(editor) = editor.upgrade() {
                        let _ = editor.edit(|profile| {
                            let mut peers = profile.peers();
                            if index < peers.len() {
                                peers.remove(index);
                            }
                            profile.set_peers(&peers);
                            Ok(())
                        });
                        editor.errors.borrow_mut().clear();
                        editor.queue_rebuild();
                    }
                }
            });
            group.append(&remove);
        }
        let (add, _) = icon_button(&self.theme, "add", true, &tr("Add peer"));
        add.set_halign(gtk4::Align::Start);
        add.connect_clicked({
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    let _ = editor.edit(|profile| {
                        let mut peers = profile.peers();
                        peers.push(Peer::default());
                        profile.set_peers(&peers);
                        Ok(())
                    });
                    editor.queue_rebuild();
                }
            }
        });
        section.append(&add);
    }

    fn openvpn(self: &Rc<Self>, page: &Page) {
        let section = self.section("vpn_key", &tr("OpenVPN"));
        let vpn = self.draft.borrow().openvpn();
        self.field(&section, &tr("Gateway"), &vpn.gateway, |text, profile| {
            edit_openvpn(profile, |vpn| vpn.gateway = text.to_owned());
            Ok(())
        });
        self.spin(
            &section,
            "numbers",
            &tr("Port (0 is the default, 1194)"),
            (0, PORT_MAX),
            vpn.port as i64,
            |profile, port| edit_openvpn(profile, |vpn| vpn.port = port as u32),
        );
        self.switch(
            &section,
            "swap_horiz",
            &tr("Use TCP"),
            |profile| profile.openvpn().tcp,
            |profile, on| edit_openvpn(profile, |vpn| vpn.tcp = on),
        );
        let kind = self.subsection(page, &section, &tr("Authentication"), "");
        self.choose(
            &kind,
            choices(&OPENVPN_KINDS),
            Value::from(vpn.kind.as_str()),
            |profile, value| {
                let kind = value.as_str().unwrap_or("tls").to_owned();
                edit_openvpn(profile, |vpn| vpn.kind = kind);
            },
        );
        if !vpn.static_key() {
            self.certificate_field(&section, &tr("CA certificate"), &vpn.ca, |profile, path| {
                edit_openvpn(profile, |vpn| vpn.ca = path)
            });
        }
        if vpn.certificates() {
            self.certificate_field(
                &section,
                &tr("User certificate"),
                &vpn.cert,
                |profile, path| edit_openvpn(profile, |vpn| vpn.cert = path),
            );
            self.certificate_field(&section, &tr("Private key"), &vpn.key, |profile, path| {
                edit_openvpn(profile, |vpn| vpn.key = path)
            });
            self.secret_field(
                &section,
                &tr("Private key password"),
                &vpn.key_password,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.key_password = text.to_owned());
                    Ok(())
                },
            );
        }
        if vpn.password() {
            self.field(
                &section,
                &tr("User name"),
                &vpn.username,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.username = text.to_owned());
                    Ok(())
                },
            );
            self.secret_field(&section, &tr("Password"), &vpn.password, |text, profile| {
                edit_openvpn(profile, |vpn| vpn.password = text.to_owned());
                Ok(())
            });
        }
        if vpn.static_key() {
            self.certificate_field(
                &section,
                &tr("Static key"),
                &vpn.static_key,
                |profile, path| edit_openvpn(profile, |vpn| vpn.static_key = path),
            );
            let direction = self.subsection(page, &section, &tr("Key direction"), "");
            self.choose(
                &direction,
                choices(&KEY_DIRECTIONS),
                Value::from(vpn.static_key_direction.as_str()),
                |profile, value| {
                    let direction = value.as_str().unwrap_or_default().to_owned();
                    edit_openvpn(profile, |vpn| vpn.static_key_direction = direction);
                },
            );
            self.field(
                &section,
                &tr("Remote tunnel address"),
                &vpn.remote_ip,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.remote_ip = text.to_owned());
                    Ok(())
                },
            );
            self.field(
                &section,
                &tr("Local tunnel address"),
                &vpn.local_ip,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.local_ip = text.to_owned());
                    Ok(())
                },
            );
            return;
        }
        let shared = self.subsection(
            page,
            &section,
            &tr("TLS authentication"),
            &tr("A key shared by the server and every client that signs the TLS handshake, the tls-auth line of an OpenVPN config"),
        );
        self.certificate_field(
            &shared,
            &tr("TLS authentication key (optional)"),
            &vpn.tls_auth,
            |profile, path| edit_openvpn(profile, |vpn| vpn.tls_auth = path),
        );
        if !vpn.tls_auth.is_empty() {
            self.choose(
                &shared,
                choices(&KEY_DIRECTIONS),
                Value::from(vpn.tls_auth_direction.as_str()),
                |profile, value| {
                    let direction = value.as_str().unwrap_or_default().to_owned();
                    edit_openvpn(profile, |vpn| vpn.tls_auth_direction = direction);
                },
            );
        }
    }

    fn vpn(self: &Rc<Self>, page: &Page) {
        if self.draft.borrow().is_openvpn() {
            self.openvpn(page);
            return;
        }
        let section = self.section("vpn_key", &tr("VPN"));
        let profile = self.draft.borrow().clone();
        let service = note();
        let plugin = profile
            .vpn_service()
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_owned();
        show_note(&service, Some(&trf("Plugin: %1", &[&plugin])), "colSubtext");
        section.append(&service);
        for (key, value) in profile.vpn_data() {
            self.field(&section, &key, &value, {
                let key = key.clone();
                move |text, profile| {
                    profile.set_vpn_data(&key, text.trim());
                    Ok(())
                }
            });
        }
    }

    fn actions_section(self: &Rc<Self>) {
        let section = self.section("", "");
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
        let (save, _) = icon_button(&self.theme, "save", true, &tr("Save"));
        save.connect_clicked({
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.save();
                }
            }
        });
        row.append(&save);
        let (revert, _) = icon_button(&self.theme, "undo", false, &tr("Revert"));
        revert.connect_clicked({
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    let saved = editor.saved.borrow().clone();
                    editor.draft.replace(saved);
                    let devices = editor
                        .saved_ports
                        .borrow()
                        .iter()
                        .map(|(_, device)| device.clone())
                        .collect();
                    editor.ports.replace(devices);
                    editor.errors.borrow_mut().clear();
                    editor.secrets_edited.set(false);
                    editor.outcome.replace(None);
                    editor.queue_rebuild();
                }
            }
        });
        row.append(&revert);
        if !self.fresh.get() && matches!(self.part.get(), Part::Main) {
            row.append(&windowdialog::spacer());
            let (delete, _) = icon_button(&self.theme, "delete", false, &tr("Delete"));
            delete.connect_clicked({
                let editor = Rc::downgrade(self);
                move |_| {
                    if let Some(editor) = editor.upgrade() {
                        editor.confirm_delete();
                    }
                }
            });
            row.append(&delete);
        }
        section.append(&row);
        let status = note();
        section.append(&status);
        self.actions.replace(Some(Actions {
            save,
            revert,
            status,
        }));
    }

    fn reveal_secrets(self: &Rc<Self>) {
        let uuid = self.saved.borrow().uuid();
        let editor = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let found = nmsettings::fetch_secrets(&uuid).await;
            let Some(editor) = editor.upgrade() else {
                return;
            };
            match found {
                Ok(found) => {
                    for secrets in &found {
                        editor.draft.borrow_mut().merge_secrets(secrets, false);
                        editor.saved.borrow_mut().merge_secrets(secrets, false);
                    }
                    editor.secrets.set(true);
                    editor.queue_rebuild();
                }
                Err(message) => {
                    editor.outcome.replace(Some(Err(message)));
                    editor.refresh();
                }
            }
        });
    }

    fn save(self: &Rc<Self>) {
        if self.busy.replace(true) {
            return;
        }
        self.outcome.replace(None);
        self.refresh();
        let profile = self.draft.borrow().clone();
        let fresh = self.fresh.get();
        let fill = !self.secrets.get() && self.secrets_edited.get();
        let wanted = self.ports.borrow().clone();
        let had = self.saved_ports.borrow().clone();
        let previous = self.saved.borrow().interface();
        let editor = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let uuid = profile.uuid();
            let mut written = if fresh {
                nmsettings::add(&profile).await
            } else {
                nmsettings::save(&profile, fill).await
            };
            let mut ports = had.clone();
            if written.is_ok() && matches!(profile.kind().as_str(), BRIDGE | BOND) {
                let synced = sync_ports(&profile, &previous, &wanted, &had).await;
                ports = nmsettings::ports(&[profile.interface(), uuid.clone()]).await;
                written = synced;
            }
            let stored = written.is_ok();
            let outcome = match written {
                Ok(()) if !fresh && nmsettings::active(&uuid).await => {
                    nmsettings::reactivate(&uuid)
                        .await
                        .map(|()| tr("Saved and reconnected"))
                }
                Ok(()) => Ok(tr("Saved")),
                Err(message) => Err(message),
            };
            let Some(editor) = editor.upgrade() else {
                return;
            };
            editor.busy.set(false);
            editor
                .ports
                .replace(ports.iter().map(|(_, device)| device.clone()).collect());
            editor.saved_ports.replace(ports);
            if stored {
                editor.saved.replace(profile);
                editor.fresh.set(false);
                editor.secrets_edited.set(false);
                editor.retitle();
            }
            editor.outcome.replace(Some(outcome));
            if fresh && stored {
                editor.queue_rebuild();
            } else {
                editor.refresh();
            }
        });
    }

    fn confirm_delete(self: &Rc<Self>) {
        let name = self.saved.borrow().id();
        let dialog = WindowDialog::new(&self.theme, None);
        dialog.set_background_width(DIALOG_WIDTH);
        dialog.column.add(
            &windowdialog::title(&trf("Delete %1?", &[&name])),
            Place::wide(),
        );
        let description =
            text::styled(&tr("Its settings and saved passwords are removed for good"));
        text::set_color(&description, "colOnSurfaceVariant");
        description.set_wrap(true);
        description.set_xalign(0.0);
        dialog.column.add(&description, Place::wide());
        let (buttons, place) = windowdialog::button_row();
        buttons.append(&windowdialog::spacer());
        let cancel = windowdialog::button(&self.theme, &tr("Cancel"));
        cancel.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        buttons.append(&cancel);
        let delete = windowdialog::button(&self.theme, &tr("Delete"));
        delete.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                let uuid = editor.saved.borrow().uuid();
                let ports = editor.saved_ports.borrow().clone();
                let editor = Rc::downgrade(&editor);
                glib::spawn_future_local(async move {
                    for (port, _) in &ports {
                        let _ = nmsettings::delete(port).await;
                    }
                    let deleted = nmsettings::delete(&uuid).await;
                    let Some(editor) = editor.upgrade() else {
                        return;
                    };
                    match deleted {
                        Ok(()) => (editor.back)(),
                        Err(message) => {
                            editor.outcome.replace(Some(Err(message)));
                            editor.refresh();
                        }
                    }
                });
            }
        });
        buttons.append(&delete);
        dialog.column.add(&buttons, place);
        (self.present)(dialog);
    }
}
