use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::process::detach;
use crate::core::scope::Scope;
use crate::panels::settings::Settings;
use crate::panels::sidebar::toggles::{self, Menu};
use crate::services::Services;
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::customicon;
use crate::ui::widgets::group::{ButtonGroup, GroupButton};
use crate::ui::widgets::text;
use crate::ui::widgets::tooltip::{self, Tooltip};

const SIZE: f64 = 40.0;
const CLICKED: f64 = SIZE + 20.0;
const ICON: f64 = 22.0;
const SPACING: f64 = 5.0;
const PADDING: f64 = 5.0;
const KINDS: [&str; 7] = [
    "network",
    "bluetooth",
    "nightLight",
    "idleInhibitor",
    "easyEffects",
    "cloudflareWarp",
    "wireGuard",
];

struct Toggle {
    kind: &'static str,
    button: GroupButton,
    glyph: gtk4::Widget,
    tooltip: Rc<Tooltip>,
}

pub struct Context {
    pub services: Rc<Services>,
    pub settings: Rc<Settings>,
    pub close: Rc<dyn Fn()>,
    pub open_menu: Rc<dyn Fn(Menu)>,
}

pub fn build(theme: &SharedTheme, context: Rc<Context>, scope: &Scope) -> gtk4::Widget {
    let group = ButtonGroup::new(theme);
    group.set_spacing(SPACING);
    group.set_padding(PADDING);
    group.set_colour(|theme| theme.colors.col_layer1);
    group.set_halign(gtk4::Align::Center);

    let toggles: Rc<Vec<Toggle>> = Rc::new(
        KINDS
            .iter()
            .map(|&kind| {
                let button = GroupButton::new(theme, SIZE, SIZE);
                button.set_clicked_width(CLICKED);
                let glyph: gtk4::Widget = match kind {
                    "cloudflareWarp" => customicon::build("cloudflare-dns-symbolic", 16).upcast(),
                    "wireGuard" => customicon::build("wireguard-symbolic", 24).upcast(),
                    _ => text::symbol("", ICON).upcast(),
                };
                glyph.set_halign(gtk4::Align::Center);
                glyph.set_valign(gtk4::Align::Center);
                button.set_content(&glyph);
                let tooltip = Tooltip::new(&button, theme, tooltip::Kind::Styled);
                tooltip.place_like_qt();
                button.connect_clicked({
                    let context = context.clone();
                    move || toggles::act(kind, &context.services, &context.close)
                });
                if has_alt(kind) {
                    button.connect_alt({
                        let context = context.clone();
                        move || alt(kind, &context)
                    });
                }
                group.append(&button);
                Toggle {
                    kind,
                    button,
                    glyph,
                    tooltip,
                }
            })
            .collect(),
    );

    let refresh = {
        let toggles = Rc::downgrade(&toggles);
        let services = context.services.clone();
        move || {
            let Some(toggles) = toggles.upgrade() else {
                return;
            };
            for toggle in toggles.iter() {
                show(toggle, &services);
            }
        }
    };
    refresh();
    toggles::subscribe(&context.services, scope, refresh);
    scope.hold(toggles);
    group.upcast()
}

fn has_alt(kind: &str) -> bool {
    matches!(
        kind,
        "network" | "bluetooth" | "nightLight" | "easyEffects" | "wireGuard"
    )
}

fn alt(kind: &str, context: &Context) {
    match kind {
        "network" => {
            let wired = *context.services.net.symbol.borrow() == "lan";
            context
                .settings
                .open(Some(if wired { "network" } else { "wifi" }));
            (context.close)();
        }
        "bluetooth" => {
            context.settings.open(Some("bluetooth"));
            (context.close)();
        }
        "nightLight" => {
            let session = &context.services.session;
            session.set_automatic(!session.automatic.get());
        }
        "easyEffects" => {
            detach(&[
                "bash",
                "-c",
                "flatpak run com.github.wwmm.easyeffects || easyeffects",
            ]);
            (context.close)();
        }
        "wireGuard" => (context.open_menu)(Menu::WireGuard),
        _ => {}
    }
}

fn show(toggle: &Toggle, services: &Rc<Services>) {
    let look = toggles::look(toggle.kind, services);
    let visible = match toggle.kind {
        "wireGuard" => true,
        _ => look.available,
    };
    toggle.button.set_visible(visible);
    let toggled = look.toggled;
    toggle.button.set_toggled(toggled);
    let radius = if has_alt(toggle.kind) && toggled {
        rounding::NORMAL as f64
    } else {
        SIZE / 2.0
    };
    toggle.button.set_radii(radius, rounding::SMALL as f64);

    let token = if toggled {
        "m3onPrimary"
    } else {
        "colOnLayer1"
    };
    text::set_color(&toggle.glyph, token);
    if let Some(label) = toggle.glyph.downcast_ref::<gtk4::Label>() {
        let icon = match toggle.kind {
            "nightLight" if services.session.automatic.get() => "night_sight_auto".to_owned(),
            "nightLight" => "bedtime".to_owned(),
            "idleInhibitor" => "coffee".to_owned(),
            "easyEffects" => "instant_mix".to_owned(),
            _ => look.icon.clone(),
        };
        label.set_text(&icon);
        text::set_symbol_font(label, ICON, if toggled { 1.0 } else { 0.0 });
    }

    let tooltip = match toggle.kind {
        "nightLight" => "Night Light | Right-click to toggle Auto mode".to_owned(),
        "idleInhibitor" => "Keep system awake".to_owned(),
        "wireGuard" => "WireGuard | Right-click to manage connections".to_owned(),
        _ => look.tooltip.clone(),
    };
    toggle.tooltip.set_text(&tooltip);
}
