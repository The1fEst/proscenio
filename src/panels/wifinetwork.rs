use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::services::wifi::{AccessPoint, State, Wifi};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::icon_button;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::{Style, TextField};
use crate::ui::widgets::windowdialog::{PADDING, button, list_item, set_list_item_active, spacer};

const ICON: f64 = pixel_size::LARGER as f64;
const ROW_SPACING: i32 = 10;
const VERTICAL_PADDING: i32 = 12;
const PROMPT_TOP: i32 = 8;
const PROMPT_SPACING: i32 = 5;

#[derive(Clone, Copy)]
pub struct Options {
    pub show_actions: bool,
    pub height: Option<(i32, i32)>,
}

pub struct NetworkList {
    pub root: gtk4::Box,
    wifi: Rc<Wifi>,
    theme: SharedTheme,
    options: Options,
    items: RefCell<Vec<Rc<Item>>>,
}

impl NetworkList {
    pub fn new(theme: &SharedTheme, wifi: &Rc<Wifi>, options: Options) -> Rc<Self> {
        let list = Rc::new(NetworkList {
            root: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            wifi: wifi.clone(),
            theme: theme.clone(),
            options,
            items: RefCell::new(Vec::new()),
        });
        let weak = Rc::downgrade(&list);
        wifi.connect_changed(move || {
            if let Some(list) = weak.upgrade() {
                list.update();
            }
        });
        list.update();
        list
    }

    fn update(&self) {
        let state = self.wifi.state.borrow();
        let previous = self.items.replace(Vec::new());
        let mut items = Vec::with_capacity(state.networks.len());
        for point in &state.networks {
            let item = previous
                .iter()
                .find(|item| item.point.borrow().ssid == point.ssid)
                .cloned()
                .unwrap_or_else(|| Item::new(&self.theme, &self.wifi, point, self.options));
            item.update(point, &state);
            items.push(item);
        }
        let shown: Vec<gtk4::Widget> = {
            let mut shown = Vec::new();
            let mut child = self.root.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                shown.push(widget);
            }
            shown
        };
        let wanted: Vec<gtk4::Widget> = items
            .iter()
            .map(|item| item.root.clone().upcast())
            .collect();
        if shown != wanted {
            for widget in &shown {
                self.root.remove(widget);
            }
            for widget in &wanted {
                self.root.append(widget);
            }
        }
        self.items.replace(items);
    }
}

struct Item {
    root: RippleButton,
    point: RefCell<AccessPoint>,
    options: Options,
    strength: gtk4::Label,
    name: gtk4::Label,
    disconnect: RippleButton,
    forget: RippleButton,
    status: gtk4::Label,
    prompt: gtk4::Box,
    password: Rc<TextField>,
}

impl Item {
    fn new(
        theme: &SharedTheme,
        wifi: &Rc<Wifi>,
        point: &AccessPoint,
        options: Options,
    ) -> Rc<Self> {
        let root = list_item(theme, point.active);
        root.set_click_phase(gtk4::PropagationPhase::Bubble);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
        let strength = text::symbol("", ICON);
        text::set_color(&strength, "colOnSurfaceVariant");
        row.append(&Centred::integral(&strength));
        let name = text::styled(&point.ssid);
        text::set_color(&name, "colOnSurfaceVariant");
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        name.set_xalign(0.0);
        let name_box = Centred::filling_width(&name);
        name_box.set_hexpand(true);
        row.append(&name_box);
        let (disconnect, _) = icon_button(theme, "link_off", true, &tr("Disconnect"));
        row.append(&disconnect);
        let (forget, _) = icon_button(theme, "delete", true, &tr("Forget"));
        row.append(&forget);
        let status = text::symbol("", ICON);
        text::set_color(&status, "colOnSurfaceVariant");
        row.append(&Centred::integral(&status));

        let prompt = gtk4::Box::new(gtk4::Orientation::Vertical, PROMPT_SPACING);
        prompt.set_margin_top(PROMPT_TOP);
        let password = TextField::secret(theme, Style::Outlined, &tr("Password"));
        password.root.set_hexpand(true);
        prompt.append(&password.root);
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, PROMPT_SPACING);
        actions.append(&spacer());
        let cancel = button(theme, &tr("Cancel"));
        let confirm = button(theme, &tr("Connect"));
        confirm.set_sensitive(false);
        password.connect_changed({
            let confirm = confirm.clone();
            let password = Rc::downgrade(&password);
            move || {
                if let Some(password) = password.upgrade() {
                    confirm.set_sensitive(!password.text().is_empty());
                }
            }
        });
        actions.append(&cancel);
        actions.append(&confirm);
        prompt.append(&actions);

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        column.append(&row);
        column.append(&prompt);
        if options.height.is_some() {
            column.set_valign(gtk4::Align::Start);
        }
        root.set_content(&column, PADDING as i32, VERTICAL_PADDING);

        let item = Rc::new(Item {
            root,
            point: RefCell::new(point.clone()),
            options,
            strength,
            name,
            disconnect,
            forget,
            status,
            prompt,
            password,
        });

        let join = {
            let weak = Rc::downgrade(&item);
            let wifi = wifi.clone();
            move || {
                if let Some(item) = weak.upgrade() {
                    let point = item.point.borrow().clone();
                    wifi.connect_with_password(&point, &item.password.text());
                }
            }
        };
        let join = Rc::new(join);
        item.password.connect_accepted({
            let join = join.clone();
            move || join()
        });
        confirm.connect_clicked(move |_| join());
        cancel.connect_clicked({
            let wifi = wifi.clone();
            move |_| wifi.cancel_password()
        });
        item.root.connect_clicked({
            let weak = Rc::downgrade(&item);
            let wifi = wifi.clone();
            move |_| {
                if let Some(item) = weak.upgrade() {
                    let point = item.point.borrow().clone();
                    wifi.connect(&point);
                }
            }
        });
        item.disconnect.connect_clicked({
            let wifi = wifi.clone();
            move |_| wifi.disconnect()
        });
        item.forget.connect_clicked({
            let weak = Rc::downgrade(&item);
            let wifi = wifi.clone();
            move |_| {
                if let Some(item) = weak.upgrade() {
                    let ssid = item.point.borrow().ssid.clone();
                    let uuid = wifi
                        .state
                        .borrow()
                        .profile(&ssid)
                        .map(|saved| saved.uuid.clone());
                    if let Some(uuid) = uuid {
                        wifi.forget(&uuid);
                    }
                }
            }
        });
        item
    }

    fn update(&self, point: &AccessPoint, state: &State) {
        self.point.replace(point.clone());
        let asking = state.asking.as_deref() == Some(point.ssid.as_str());
        let targeted = state.target.as_deref() == Some(point.ssid.as_str());
        set_list_item_active(&self.root, asking || point.active);
        self.root.set_sensitive(!(targeted && !point.active));
        self.strength.set_text(strength_symbol(point.strength));
        self.name.set_text(&point.ssid);
        self.disconnect
            .set_visible(self.options.show_actions && point.active);
        self.forget
            .set_visible(self.options.show_actions && state.is_saved(&point.ssid));
        self.status.set_visible(point.secure() || point.active);
        self.status.set_text(if point.active {
            "check"
        } else if targeted {
            "settings_ethernet"
        } else {
            "lock"
        });
        self.prompt.set_visible(asking);
        if let Some((resting, prompting)) = self.options.height {
            self.root
                .set_size_request(-1, if asking { resting + prompting } else { resting });
        }
    }
}

fn strength_symbol(strength: i32) -> &'static str {
    match strength {
        81.. => "signal_wifi_4_bar",
        61..=80 => "network_wifi_3_bar",
        41..=60 => "network_wifi_2_bar",
        21..=40 => "network_wifi_1_bar",
        _ => "signal_wifi_0_bar",
    }
}
