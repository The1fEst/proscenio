use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::platform::security::pam;
use crate::services::network::wifi::{AccessPoint, State, Wifi};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::icon_button;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::textfield::{Style, TextField};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::windowdialog::{
    PADDING, button, list_item, set_list_item_active, spacer,
};

const ICON: f64 = pixel_size::LARGER as f64;
const ROW_SPACING: i32 = 10;
const VERTICAL_PADDING: i32 = 12;
const PROMPT_TOP: i32 = 8;
const PROMPT_SPACING: i32 = 5;

#[derive(Clone, Copy)]
pub struct Options {
    pub shows_actions: bool,
    pub heights: Option<(i32, i32)>,
}

pub struct NetworkList {
    pub root: gtk4::Box,
    wifi: Rc<Wifi>,
    theme: SharedTheme,
    options: Options,
    on_enterprise: Rc<dyn Fn(&str)>,
    items: RefCell<Vec<Rc<Item>>>,
}

impl NetworkList {
    pub fn new(
        theme: &SharedTheme,
        wifi: &Rc<Wifi>,
        options: Options,
        on_enterprise: impl Fn(&str) + 'static,
    ) -> Rc<Self> {
        let list = Rc::new(NetworkList {
            root: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            wifi: wifi.clone(),
            theme: theme.clone(),
            options,
            on_enterprise: Rc::new(on_enterprise),
            items: RefCell::new(Vec::new()),
        });
        let weak_list = Rc::downgrade(&list);
        wifi.connect_changed(move || {
            if let Some(list) = weak_list.upgrade() {
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
                .unwrap_or_else(|| {
                    Item::new(
                        &self.theme,
                        &self.wifi,
                        point,
                        self.options,
                        self.on_enterprise.clone(),
                    )
                });
            item.update(point, &state);
            items.push(item);
        }
        let current_children: Vec<gtk4::Widget> = {
            let mut children = Vec::new();
            let mut child = self.root.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                children.push(widget);
            }
            children
        };
        let target_children: Vec<gtk4::Widget> = items
            .iter()
            .map(|item| item.root.clone().upcast())
            .collect();
        if current_children != target_children {
            for widget in &current_children {
                self.root.remove(widget);
            }
            for widget in &target_children {
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
    strength_label: gtk4::Label,
    name_label: gtk4::Label,
    disconnect_button: RippleButton,
    forget_button: RippleButton,
    status_label: gtk4::Label,
    prompt_box: gtk4::Box,
    password_field: Rc<TextField>,
}

impl Item {
    fn new(
        theme: &SharedTheme,
        wifi: &Rc<Wifi>,
        point: &AccessPoint,
        options: Options,
        on_enterprise: Rc<dyn Fn(&str)>,
    ) -> Rc<Self> {
        let root = list_item(theme, point.is_active);
        root.set_click_phase(gtk4::PropagationPhase::Bubble);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
        let strength_label = text::symbol("", ICON);
        text::set_color(&strength_label, "colOnSurfaceVariant");
        row.append(&Centered::integral(&strength_label));
        let name_label = text::styled(&point.ssid);
        text::set_color(&name_label, "colOnSurfaceVariant");
        name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        name_label.set_xalign(0.0);
        let name_box = Centered::filling_width(&name_label);
        name_box.set_hexpand(true);
        row.append(&name_box);
        let (disconnect_button, _) = icon_button(theme, "link_off", true, &tr("Disconnect"));
        row.append(&disconnect_button);
        let (forget_button, _) = icon_button(theme, "delete", true, &tr("Forget"));
        row.append(&forget_button);
        let status_label = text::symbol("", ICON);
        text::set_color(&status_label, "colOnSurfaceVariant");
        row.append(&Centered::integral(&status_label));

        let prompt_box = gtk4::Box::new(gtk4::Orientation::Vertical, PROMPT_SPACING);
        prompt_box.set_margin_top(PROMPT_TOP);
        let password_field = TextField::secret(theme, Style::Outlined, &tr("Password"));
        password_field.root.set_hexpand(true);
        prompt_box.append(&password_field.root);
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, PROMPT_SPACING);
        actions.append(&spacer());
        let cancel_button = button(theme, &tr("Cancel"));
        let confirm_button = button(theme, &tr("Connect"));
        confirm_button.set_sensitive(false);
        password_field.connect_changed({
            let confirm_button = confirm_button.clone();
            let password_field = Rc::downgrade(&password_field);
            move || {
                if let Some(password_field) = password_field.upgrade() {
                    confirm_button.set_sensitive(!password_field.is_empty());
                }
            }
        });
        actions.append(&cancel_button);
        actions.append(&confirm_button);
        prompt_box.append(&actions);

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        column.append(&row);
        column.append(&prompt_box);
        if options.heights.is_some() {
            column.set_valign(gtk4::Align::Start);
        }
        root.set_content(&column, PADDING as i32, VERTICAL_PADDING);

        let item = Rc::new(Item {
            root,
            point: RefCell::new(point.clone()),
            options,
            strength_label,
            name_label,
            disconnect_button,
            forget_button,
            status_label,
            prompt_box,
            password_field,
        });

        let join = {
            let weak_item = Rc::downgrade(&item);
            let wifi = wifi.clone();
            move || {
                if let Some(item) = weak_item.upgrade() {
                    let point = item.point.borrow().clone();
                    let password = item.password_field.text();
                    wifi.connect_with_password(&point, &password);
                    pam::wipe(&mut password.into_bytes());
                }
            }
        };
        let join = Rc::new(join);
        item.password_field.connect_accepted({
            let join = join.clone();
            move || join()
        });
        confirm_button.connect_clicked(move |_| join());
        cancel_button.connect_clicked({
            let wifi = wifi.clone();
            move |_| wifi.cancel_password()
        });
        item.root.connect_clicked({
            let weak_item = Rc::downgrade(&item);
            let wifi = wifi.clone();
            move |_| {
                if let Some(item) = weak_item.upgrade() {
                    let point = item.point.borrow().clone();
                    let is_saved = wifi.state.borrow().is_saved(&point.ssid);
                    if point.is_enterprise() && !is_saved {
                        on_enterprise(&point.ssid);
                    } else {
                        wifi.connect(&point);
                    }
                }
            }
        });
        item.disconnect_button.connect_clicked({
            let wifi = wifi.clone();
            move |_| wifi.disconnect()
        });
        item.forget_button.connect_clicked({
            let weak_item = Rc::downgrade(&item);
            let wifi = wifi.clone();
            move |_| {
                if let Some(item) = weak_item.upgrade() {
                    let ssid = item.point.borrow().ssid.clone();
                    let uuid = wifi
                        .state
                        .borrow()
                        .saved_network(&ssid)
                        .map(|network| network.uuid.clone());
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
        let is_asking = state.password_prompt_ssid.as_deref() == Some(point.ssid.as_str());
        let is_targeted = state.target_ssid.as_deref() == Some(point.ssid.as_str());
        set_list_item_active(&self.root, is_asking || point.is_active);
        self.root.set_sensitive(!(is_targeted && !point.is_active));
        self.strength_label
            .set_text(strength_symbol(point.strength));
        self.name_label.set_text(&point.ssid);
        self.disconnect_button
            .set_visible(self.options.shows_actions && point.is_active);
        self.forget_button
            .set_visible(self.options.shows_actions && state.is_saved(&point.ssid));
        self.status_label
            .set_visible(point.is_secure() || point.is_active);
        self.status_label.set_text(if point.is_active {
            "check"
        } else if is_targeted {
            "settings_ethernet"
        } else {
            "lock"
        });
        self.prompt_box.set_visible(is_asking);
        if let Some((resting_height, prompting_height)) = self.options.heights {
            self.root.set_size_request(
                -1,
                if is_asking {
                    resting_height + prompting_height
                } else {
                    resting_height
                },
            );
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
