use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{i18n, process};
use crate::panels::settings::content::{Context, Page};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let main = page.section("", "");
    let language = page.subsection(
        &main,
        &tr("Language"),
        &tr("Select the language for the user interface.\n\"Auto\" will use your system's locale."),
    );
    let languages = page.combo(&language, "language");
    let mut codes = vec![i18n::AUTO];
    codes.extend(i18n::available());
    let names: Vec<String> = codes
        .iter()
        .map(|code| match *code {
            i18n::AUTO => tr("Auto (System)"),
            code => i18n::display_name(code),
        })
        .collect();
    let chosen = i18n::chosen();
    let current = codes
        .iter()
        .position(|code| *code == chosen)
        .map_or_else(String::new, |index| names[index].clone());
    languages.set_items_showing(&names, &current);
    languages.connect_activated(move |index| {
        if codes[index] == chosen {
            return;
        }
        i18n::choose(codes[index], || {
            process::restart_shell_on_settings("region")
        });
    });

    page
}
