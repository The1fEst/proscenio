use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, process};
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::administration::privacy;
use crate::services::hardware::shellusage::Meter;
use crate::ui::widgets::input::selection::Choice;

const USAGE_POLL: std::time::Duration = std::time::Duration::from_secs(2);

const RENDERER_TIPS: [&str; 3] = [
    "Draws on the processor. Uses no graphics memory and works on any system, so it is the default; animations and large panels cost more processor time",
    "Draws on the graphics card through OpenGL. Animations are smoother and the processor stays idle, at the cost of some graphics memory for every open surface",
    "Draws on the graphics card through Vulkan. Usually the lightest on the processor and the smoothest, but it needs a working Vulkan driver",
];

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let workarounds_section = page.section("bug_report", &tr("Workarounds"));
    let dead_pixel_switch = page.config_switch(
        &workarounds_section,
        "border_outer",
        &tr("Dead pixel workaround"),
        "/interactions/deadPixelWorkaround/enable",
        false,
    );
    page.tip(
        &dead_pixel_switch.button,
        &tr("Hyprland leaves out one pixel on the right and bottom edges for interactions. Enable if screen corners don't react to your cursor."),
    );
    let (delay_row, _) = page.config_spin(
        &workarounds_section,
        "hourglass",
        &tr("Race condition delay (ms)"),
        "/hacks/arbitraryRaceConditionDelay",
        20,
        (0, 500),
        5,
    );
    page.tip(
        &delay_row,
        &tr("Increase if things occasionally show up in the wrong place or size on a slow system"),
    );

    let rendering_section = page.section("brush", &tr("Rendering"));
    let renderer_section = page.subsection(
        &rendering_section,
        &tr("Renderer"),
        &tr("After the restart a window asks whether to keep the new renderer, and the previous one comes back unless you keep it within 15 seconds. `proscenio ipc call renderer reset` returns to Cairo from a terminal. A GSK_RENDERER set in the environment wins over this choice."),
    );
    let choice = |label: &str, icon: &'static str, value: &str| Choice {
        label: label.to_owned(),
        icon,
        value: Value::from(value),
    };
    let renderer_selection = page.selection(
        &renderer_section,
        vec![
            choice("Cairo", "brush", "cairo"),
            choice("OpenGL", "view_in_ar", "opengl"),
            choice("Vulkan", "bolt", "vulkan"),
        ],
        config::RENDERER,
        Value::from(config::DEFAULT_RENDERER),
        |value| {
            if let Some(renderer) = value.as_str() {
                config::pick_renderer(renderer);
            }
        },
    );
    for (index, tip) in RENDERER_TIPS.iter().enumerate() {
        if let Some(button) = renderer_selection.button(index) {
            page.tip(button, &tr(tip));
        }
    }
    let restart_notice = page.notice(
        &rendering_section,
        "restart_alt",
        &tr("The shell tries the new renderer once it restarts."),
    );
    let (restart_button, _) = page.icon_button("restart_alt", true, &tr("Restart shell"), || {
        process::restart_shell_on_settings("advanced")
    });
    restart_button.set_valign(gtk4::Align::Center);
    restart_notice.append(&restart_button);
    let update_restart_notice = move || {
        let selected_renderer = config::value_str(config::RENDERER)
            .unwrap_or_else(|| config::DEFAULT_RENDERER.to_owned());
        restart_notice.set_visible(selected_renderer != config::active_renderer());
    };
    update_restart_notice();
    page.watch(config::RENDERER, update_restart_notice);

    let usage_section = page.section("monitor_heart", &tr("Shell usage"));
    let top_row = page.uniform_row(&usage_section);
    let cpu_tile = privacy::tile(&top_row, "memory", "");
    let memory_tile = privacy::tile(&top_row, "memory_alt", "");
    let bottom_row = page.uniform_row(&usage_section);
    let gpu_tile = privacy::tile(&bottom_row, "developer_board", "");
    let video_tile = privacy::tile(&bottom_row, "view_in_ar", "");
    let labels = Rc::new([
        cpu_tile.label,
        memory_tile.label,
        gpu_tile.label,
        video_tile.label,
    ]);
    let meter = Meter::new();
    page.every(USAGE_POLL, move || {
        let labels = labels.clone();
        meter.sample(move |usage| {
            let [cpu_label, memory_label, gpu_label, video_label] = labels.as_ref();
            cpu_label.set_text(&format!("{} {}", tr("CPU"), percent(usage.cpu_percent)));
            memory_label.set_text(&format!(
                "{} {}",
                tr("Memory"),
                format_mib(usage.memory_bytes)
            ));
            gpu_label.set_text(&format!("{} {}", tr("GPU"), percent(usage.gpu_percent)));
            video_label.set_text(&format!(
                "{} {}",
                tr("Video memory"),
                format_mib(usage.video_bytes)
            ));
        });
    });
    page
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.1} %"))
}

fn format_mib(value: Option<u64>) -> String {
    value.map_or_else(
        || "—".to_owned(),
        |value| format!("{:.0} MiB", value as f64 / (1024.0 * 1024.0)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_shown_in_mib() {
        assert_eq!(format_mib(Some(300 << 20)), "300 MiB");
    }
}
