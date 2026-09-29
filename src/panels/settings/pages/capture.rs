use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, tools};
use crate::panels::settings::content::{Context, Page, Style};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let paths = page.section("folder", &tr("Save paths"));
    page.tools_notice(
        &paths,
        &[&tools::GRIM, &tools::MAGICK, &tools::WL_COPY, &tools::SATTY],
        &tr("screenshots and the snip actions that need them fail"),
    );
    page.tools_notice(
        &paths,
        &[&tools::WF_RECORDER, &tools::SLURP],
        &tr("screen recording fails"),
    );
    let screenshots = page.subsection(
        &paths,
        &tr("Screenshots"),
        &tr("A screenshot always goes to the clipboard. Turn this on to keep a file as well."),
    );
    page.config_switch(
        &screenshots,
        "save",
        &tr("Also save to a file"),
        "/screenSnip/save",
        false,
    );
    let snip_path = page.config_text(
        &screenshots,
        Style::Filled,
        &tr("e.g. ~/Pictures/Screenshots"),
        "/screenSnip/savePath",
        "",
    );
    let enable = {
        let field = Rc::downgrade(&snip_path);
        move || {
            if let Some(field) = field.upgrade() {
                field.set_enabled(config::value_bool("/screenSnip/save", false));
            }
        }
    };
    enable();
    page.watch("/screenSnip/save", enable);

    let recordings = page.subsection(&paths, &tr("Screen recordings"), "");
    let videos = gtk4::glib::user_special_dir(gtk4::glib::UserDirectory::Videos)
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    page.config_text(
        &recordings,
        Style::Filled,
        &tr("e.g. ~/Videos/Recordings"),
        "/screenRecord/savePath",
        &videos,
    );

    let selector = page.section(
        "screenshot_frame_2",
        &tr("Region selector (screen snipping)"),
    );
    page.config_switch(
        &selector,
        "arrow_selector_tool",
        &tr("Include the pointer"),
        "/regionSelector/showPointer",
        false,
    );

    let hints = page.subsection(&selector, &tr("Hint target regions"), "");
    let kinds = page.row(&hints);
    page.config_switch(
        &kinds,
        "select_window",
        &tr("Windows"),
        "/regionSelector/targetRegions/windows",
        true,
    );
    page.config_switch(
        &kinds,
        "right_panel_open",
        &tr("Layers"),
        "/regionSelector/targetRegions/layers",
        false,
    );
    page.config_switch(
        &hints,
        "label",
        &tr("Show region labels"),
        "/regionSelector/targetRegions/showLabel",
        false,
    );
    page.config_spin_scaled(
        &hints,
        "opacity",
        &tr("Hint opacity (%)"),
        "/regionSelector/targetRegions/opacity",
        0.3,
        100.0,
        (0, 100),
        5,
    );
    page.config_spin(
        &hints,
        "padding",
        &tr("Selection padding"),
        "/regionSelector/targetRegions/selectionPadding",
        5,
        (0, 50),
        1,
    );

    let rectangle = page.subsection(&selector, &tr("Rectangular selection"), "");
    page.config_switch(
        &rectangle,
        "point_scan",
        &tr("Show aim lines"),
        "/regionSelector/rect/showAimLines",
        true,
    );

    let circle = page.subsection(&selector, &tr("Circle selection"), "");
    page.config_spin(
        &circle,
        "eraser_size_3",
        &tr("Stroke width"),
        "/regionSelector/circle/strokeWidth",
        6,
        (1, 20),
        1,
    );
    page.config_spin(
        &circle,
        "screenshot_frame_2",
        &tr("Padding"),
        "/regionSelector/circle/padding",
        10,
        (0, 100),
        5,
    );
    page
}
