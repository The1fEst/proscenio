use std::rc::Rc;

use crate::panels::settings::content::{Context, Page, Style};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let main = page.section("", "");

    let sloppy = page.config_switch(
        &main,
        "",
        "Use Levenshtein distance-based algorithm instead of fuzzy",
        "/search/sloppy",
        false,
    );
    page.tip(
        &sloppy.button,
        "Could be better if you make a ton of typos,\nbut results can be weird and might not work with acronyms\n(e.g. \"GIMP\" might not give you the paint program)",
    );

    let (delay, _) = page.config_spin(
        &main,
        "av_timer",
        "Non-app result delay (ms)",
        "/search/nonAppResultDelay",
        30,
        (0, 500),
        10,
    );
    page.tip(
        &delay,
        "Delays the expensive result types (math, commands, web) so typing stays smooth",
    );

    let prefixes = page.subsection(&main, "Prefixes", "");
    page.config_switch(
        &prefixes,
        "bolt",
        "Show default actions without a prefix",
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
        page.config_text(&first, Style::Filled, placeholder, pointer, default);
    }
    let second = page.uniform_row(&prefixes);
    for (placeholder, pointer, default) in [
        ("Math", "/search/prefix/math", "="),
        ("Shell command", "/search/prefix/shellCommand", "$"),
    ] {
        page.config_text(&second, Style::Filled, placeholder, pointer, default);
    }
    page
}
