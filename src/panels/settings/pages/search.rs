use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::tools;
use crate::panels::settings::content::{Context, Page, Style};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let main = page.section("", "");

    let sloppy = page.config_switch(
        &main,
        "",
        &tr("Use Levenshtein distance-based algorithm instead of fuzzy"),
        "/search/sloppy",
        false,
    );
    page.tip(
        &sloppy.button,
        &tr(
            "Could be better if you make a ton of typos,\nbut results can be weird and might not work with acronyms\n(e.g. \"GIMP\" might not give you the paint program)",
        ),
    );

    let (delay, _) = page.config_spin(
        &main,
        "av_timer",
        &tr("Non-app result delay (ms)"),
        "/search/nonAppResultDelay",
        30,
        (0, 500),
        10,
    );
    page.tip(
        &delay,
        &tr("Delays the expensive result types (math, commands, web) so typing stays smooth"),
    );

    let prefixes = page.subsection(&main, &tr("Prefixes"), "");
    page.tools_notice(&prefixes, &[&tools::QALC], &tr("math results never show"));
    page.tools_notice(
        &prefixes,
        &[&tools::CLIPHIST],
        &tr("the clipboard prefix finds nothing"),
    );
    page.config_switch(
        &prefixes,
        "bolt",
        &tr("Show default actions without a prefix"),
        "/search/prefix/showDefaultActionsWithoutPrefix",
        true,
    );
    let first = page.uniform_row(&prefixes);
    for (placeholder, pointer, default) in [
        ("Apps", "/search/prefix/app", ">"),
        ("Action", "/search/prefix/action", "/"),
        ("Clipboard", "/search/prefix/clipboard", ";"),
        ("Emojis", "/search/prefix/emojis", ":"),
    ] {
        page.config_text(&first, Style::Filled, &tr(placeholder), pointer, default);
    }
    let second = page.uniform_row(&prefixes);
    for (placeholder, pointer, default) in [
        ("Math", "/search/prefix/math", "="),
        ("Shell command", "/search/prefix/shellCommand", "$"),
    ] {
        page.config_text(&second, Style::Filled, &tr(placeholder), pointer, default);
    }
    page
}
