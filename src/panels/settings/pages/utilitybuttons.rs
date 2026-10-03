use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::tools;
use crate::panels::settings::content::{Context, Page};

const POWER_PROFILES: &str = "net.hadess.PowerProfiles";
const NO_POWER_PROFILES: &str = "power-profiles-daemon is not on the system bus, so the Performance Profile button does nothing. It comes with the power-profiles-daemon package.";
const UPDATES: &str = "/bar/utilButtons/showUpdates";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let buttons = page.section("", "");
    page.tools_notice(
        &buttons,
        &[
            &tools::GRIM,
            &tools::MAGICK,
            &tools::WL_COPY,
            &tools::HYPRPICKER,
            &tools::YDOTOOL,
            &tools::WPCTL,
            &tools::WF_RECORDER,
            &tools::SLURP,
        ],
        &tr("the buttons that run them do nothing"),
    );
    if !tools::system_service(POWER_PROFILES) {
        page.notice(&buttons, "info", &tr(NO_POWER_PROFILES));
    }
    for pair in [
        [
            (
                "content_cut",
                "Screen snip",
                "/bar/utilButtons/showScreenSnip",
                true,
            ),
            (
                "colorize",
                "Color picker",
                "/bar/utilButtons/showColorPicker",
                false,
            ),
        ],
        [
            (
                "keyboard",
                "Keyboard toggle",
                "/bar/utilButtons/showKeyboardToggle",
                true,
            ),
            ("mic", "Mic toggle", "/bar/utilButtons/showMicToggle", false),
        ],
        [
            (
                "dark_mode",
                "Dark/Light toggle",
                "/bar/utilButtons/showDarkModeToggle",
                true,
            ),
            (
                "speed",
                "Performance Profile toggle",
                "/bar/utilButtons/showPerformanceProfileToggle",
                false,
            ),
        ],
        [
            (
                "videocam",
                "Record",
                "/bar/utilButtons/showScreenRecord",
                false,
            ),
            ("deployed_code_update", "System updates", UPDATES, true),
        ],
    ] {
        let row = page.uniform_row(&buttons);
        for (icon, label, pointer, default) in pair {
            let switch = page.config_switch(&row, icon, &tr(label), pointer, default);
            if pointer == UPDATES {
                page.tip(
                    &switch.button,
                    &tr(
                        "Appears once enough packages are out of date. The threshold is under Services.",
                    ),
                );
            }
        }
    }
    page
}
