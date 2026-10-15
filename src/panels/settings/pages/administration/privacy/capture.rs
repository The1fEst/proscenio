use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, tools};
use crate::panels::settings::content::{Context, Page, Style};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let paths_section = page.section("folder", &tr("Save paths"));
    page.tools_notice(
        &paths_section,
        &[&tools::GRIM, &tools::MAGICK, &tools::WL_COPY, &tools::SATTY],
        &tr("screenshots and the snip actions that need them fail"),
    );
    page.tools_notice(
        &paths_section,
        &[&tools::WF_RECORDER, &tools::SLURP],
        &tr("screen recording fails"),
    );
    let screenshots_section = page.subsection(
        &paths_section,
        &tr("Screenshots"),
        &tr("A screenshot always goes to the clipboard. Turn this on to keep a file as well."),
    );
    page.config_switch(
        &screenshots_section,
        "save",
        &tr("Also save to a file"),
        "/screenSnip/save",
        false,
    );
    let snip_path_field = page.config_text(
        &screenshots_section,
        Style::Filled,
        &tr("e.g. ~/Pictures/Screenshots"),
        "/screenSnip/savePath",
        "",
    );
    let update_snip_path_field = {
        let field = Rc::downgrade(&snip_path_field);
        move || {
            if let Some(field) = field.upgrade() {
                field.set_enabled(config::value_bool("/screenSnip/save", false));
            }
        }
    };
    update_snip_path_field();
    page.watch("/screenSnip/save", update_snip_path_field);

    let recordings_section = page.subsection(&paths_section, &tr("Screen recordings"), "");
    let videos_folder = gtk4::glib::user_special_dir(gtk4::glib::UserDirectory::Videos)
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    page.config_text(
        &recordings_section,
        Style::Filled,
        &tr("e.g. ~/Videos/Recordings"),
        "/screenRecord/savePath",
        &videos_folder,
    );

    let selector_section = page.section(
        "screenshot_frame_2",
        &tr("Region selector (screen snipping)"),
    );
    page.config_switch(
        &selector_section,
        "arrow_selector_tool",
        &tr("Include the pointer"),
        "/regionSelector/showPointer",
        false,
    );

    let hints_section = page.subsection(&selector_section, &tr("Hint target regions"), "");
    let kinds_row = page.row(&hints_section);
    page.config_switch(
        &kinds_row,
        "select_window",
        &tr("Windows"),
        "/regionSelector/targetRegions/windows",
        true,
    );
    page.config_switch(
        &kinds_row,
        "right_panel_open",
        &tr("Layers"),
        "/regionSelector/targetRegions/layers",
        false,
    );
    page.config_switch(
        &hints_section,
        "label",
        &tr("Show region labels"),
        "/regionSelector/targetRegions/showLabel",
        false,
    );
    page.config_spin_scaled(
        &hints_section,
        "opacity",
        &tr("Hint opacity (%)"),
        "/regionSelector/targetRegions/opacity",
        0.3,
        100.0,
        (0, 100),
        5,
    );
    page.config_spin(
        &hints_section,
        "padding",
        &tr("Selection padding"),
        "/regionSelector/targetRegions/selectionPadding",
        5,
        (0, 50),
        1,
    );

    let rectangle_section = page.subsection(&selector_section, &tr("Rectangular selection"), "");
    page.config_switch(
        &rectangle_section,
        "point_scan",
        &tr("Show aim lines"),
        "/regionSelector/rect/showAimLines",
        true,
    );

    let circle_section = page.subsection(&selector_section, &tr("Circle selection"), "");
    page.config_spin(
        &circle_section,
        "eraser_size_3",
        &tr("Stroke width"),
        "/regionSelector/circle/strokeWidth",
        6,
        (1, 20),
        1,
    );
    page.config_spin(
        &circle_section,
        "screenshot_frame_2",
        &tr("Padding"),
        "/regionSelector/circle/padding",
        10,
        (0, 100),
        5,
    );
    page
}
