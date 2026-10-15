use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{i18n, process};
use crate::panels::settings::content::{Context, Page};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let main_section = page.section("", "");
    let language_section = page.subsection(
        &main_section,
        &tr("Language"),
        &tr("Select the language for the user interface.\n\"Auto\" will use your system's locale."),
    );
    let language_combo = page.combo(&language_section, "language");
    let mut codes = vec![i18n::AUTO];
    codes.extend(i18n::available());
    let names: Vec<String> = codes
        .iter()
        .map(|code| match *code {
            i18n::AUTO => tr("Auto (System)"),
            code => i18n::display_name(code),
        })
        .collect();
    let selected_code = i18n::language_setting();
    let selected_name = codes
        .iter()
        .position(|code| *code == selected_code)
        .map_or_else(String::new, |index| names[index].clone());
    language_combo.set_items_showing(&names, &selected_name);
    language_combo.connect_activated(move |index| {
        if codes[index] == selected_code {
            return;
        }
        i18n::choose(codes[index], || {
            process::restart_shell_on_settings("region")
        });
    });

    page
}
