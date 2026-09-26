use std::rc::Rc;

use crate::core::config;
use crate::panels::settings::content::{Context, Page, Style};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let paths = page.section("folder", "Save paths");
    let screenshots = page.subsection(
        &paths,
        "Screenshots",
        "A screenshot always goes to the clipboard. Turn this on to keep a file as well.",
    );
    page.config_switch(
        &screenshots,
        "save",
        "Also save to a file",
        "/screenSnip/save",
        false,
    );
    let snip_path = page.config_text(
        &screenshots,
        Style::Filled,
        "e.g. ~/Pictures/Screenshots",
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

    let recordings = page.subsection(&paths, "Screen recordings", "");
    let videos = gtk4::glib::user_special_dir(gtk4::glib::UserDirectory::Videos)
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    page.config_text(
        &recordings,
        Style::Filled,
        "e.g. ~/Videos/Recordings",
        "/screenRecord/savePath",
        &videos,
    );

    let selector = page.section("screenshot_frame_2", "Region selector (screen snipping)");
    page.config_switch(
        &selector,
        "arrow_selector_tool",
        "Include the pointer",
        "/regionSelector/showPointer",
        false,
    );

    let hints = page.subsection(&selector, "Hint target regions", "");
    let kinds = page.row(&hints);
    page.config_switch(
        &kinds,
        "select_window",
        "Windows",
        "/regionSelector/targetRegions/windows",
        true,
    );
    page.config_switch(
        &kinds,
        "right_panel_open",
        "Layers",
        "/regionSelector/targetRegions/layers",
        false,
    );
    page.config_switch(
        &hints,
        "label",
        "Show region labels",
        "/regionSelector/targetRegions/showLabel",
        false,
    );
    page.config_spin_scaled(
        &hints,
        "opacity",
        "Hint opacity (%)",
        "/regionSelector/targetRegions/opacity",
        0.3,
        100.0,
        (0, 100),
        5,
    );
    page.config_spin(
        &hints,
        "padding",
        "Selection padding",
        "/regionSelector/targetRegions/selectionPadding",
        5,
        (0, 50),
        1,
    );

    let rectangle = page.subsection(&selector, "Rectangular selection", "");
    page.config_switch(
        &rectangle,
        "point_scan",
        "Show aim lines",
        "/regionSelector/rect/showAimLines",
        true,
    );

    let circle = page.subsection(&selector, "Circle selection", "");
    page.config_spin(
        &circle,
        "eraser_size_3",
        "Stroke width",
        "/regionSelector/circle/strokeWidth",
        6,
        (1, 20),
        1,
    );
    page.config_spin(
        &circle,
        "screenshot_frame_2",
        "Padding",
        "/regionSelector/circle/padding",
        10,
        (0, 100),
        5,
    );
    page
}
