use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::connection::{self, Editor, PRIVACY, Part, choices};
use crate::panels::settings::pages::ipv4::{edit_ip, ip};
use crate::platform::nmprofile::Family;

pub fn build(context: &Context) -> Rc<Page> {
    connection::show(context, Part::Sub(fill))
}

fn fill(editor: &Rc<Editor>, page: &Page) {
    let section = ip(editor, page, Family::V6);
    let current = editor.draft.borrow().ip(Family::V6);
    if !matches!(current.method.as_str(), "auto" | "dhcp") {
        return;
    }
    let privacy = editor.subsection(
        page,
        &section,
        &tr("Privacy extensions"),
        &tr("Temporary addresses change over time, so sites cannot follow this device by its address"),
    );
    editor.choose(
        &privacy,
        choices(&PRIVACY),
        Value::from(current.privacy),
        |profile, value| {
            let privacy = value.as_i64().unwrap_or(-1) as i32;
            edit_ip(profile, Family::V6, &|ip| ip.privacy = privacy);
        },
    );
}
