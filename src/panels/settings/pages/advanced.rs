use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::{config, process};
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::privacy;
use crate::services::shellusage::Meter;
use crate::ui::widgets::selection::Choice;

const USAGE_POLL: std::time::Duration = std::time::Duration::from_secs(2);

const RENDERER_TIPS: [&str; 3] = [
    "Draws on the processor. Uses no graphics memory and works on any system, so it is the default; animations and large panels cost more processor time",
    "Draws on the graphics card through OpenGL. Animations are smoother and the processor stays idle, at the cost of some graphics memory for every open surface",
    "Draws on the graphics card through Vulkan. Usually the lightest on the processor and the smoothest, but it needs a working Vulkan driver",
];

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let scrolling = page.section("swipe", "Scrolling");
    page.config_switch(
        &scrolling,
        "touch_app",
        "Faster touchpad scrolling",
        "/interactions/scrolling/fasterTouchpadScroll",
        false,
    );
    page.config_spin(
        &scrolling,
        "mouse",
        "Mouse scroll distance",
        "/interactions/scrolling/mouseScrollFactor",
        120,
        (10, 1000),
        10,
    );
    page.config_spin(
        &scrolling,
        "touchpad_mouse",
        "Touchpad scroll distance",
        "/interactions/scrolling/touchpadScrollFactor",
        450,
        (10, 1000),
        10,
    );
    let (threshold, _) = page.config_spin(
        &scrolling,
        "conversion_path",
        "Mouse detection threshold",
        "/interactions/scrolling/mouseScrollDeltaThreshold",
        120,
        (1, 500),
        10,
    );
    page.tip(
        &threshold,
        "Scroll events at least this large are treated as coming from a mouse instead of a touchpad",
    );

    let workarounds = page.section("bug_report", "Workarounds");
    let dead_pixel = page.config_switch(
        &workarounds,
        "border_outer",
        "Dead pixel workaround",
        "/interactions/deadPixelWorkaround/enable",
        false,
    );
    page.tip(
        &dead_pixel.button,
        "Hyprland leaves out one pixel on the right and bottom edges for interactions. Enable if screen corners don't react to your cursor.",
    );
    let (delay, _) = page.config_spin(
        &workarounds,
        "hourglass",
        "Race condition delay (ms)",
        "/hacks/arbitraryRaceConditionDelay",
        20,
        (0, 500),
        5,
    );
    page.tip(
        &delay,
        "Increase if things occasionally show up in the wrong place or size on a slow system",
    );

    let rendering = page.section("brush", "Rendering");
    let renderer = page.subsection(
        &rendering,
        "Renderer",
        "After the restart a window asks whether to keep the new renderer, and the previous one comes back unless you keep it within 15 seconds. `proscenio ipc call renderer reset` returns to Cairo from a terminal. A GSK_RENDERER set in the environment wins over this choice.",
    );
    let choice = |label: &str, icon: &'static str, value: &str| Choice {
        label: label.to_owned(),
        icon,
        value: Value::from(value),
    };
    let renderers = page.selection(
        &renderer,
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
        if let Some(button) = renderers.button(index) {
            page.tip(button, tip);
        }
    }
    let pending = page.notice(
        &rendering,
        "restart_alt",
        "The shell tries the new renderer once it restarts.",
    );
    let (restart, _) = page.icon_button("restart_alt", true, "Restart shell", || {
        process::restart_shell_on_settings("advanced")
    });
    restart.set_valign(gtk4::Align::Center);
    pending.append(&restart);
    let follow = move || {
        let chosen = config::value_str(config::RENDERER)
            .unwrap_or_else(|| config::DEFAULT_RENDERER.to_owned());
        pending.set_visible(chosen != config::running_renderer());
    };
    follow();
    page.watch(config::RENDERER, follow);

    let usage = page.section("monitor_heart", "Shell usage");
    let top = page.uniform_row(&usage);
    let cpu = privacy::tile(&top, "memory", "");
    let memory = privacy::tile(&top, "memory_alt", "");
    let bottom = page.uniform_row(&usage);
    let gpu = privacy::tile(&bottom, "developer_board", "");
    let video = privacy::tile(&bottom, "view_in_ar", "");
    let tiles = Rc::new([cpu.label, memory.label, gpu.label, video.label]);
    let meter = Meter::new();
    page.every(USAGE_POLL, move || {
        let tiles = tiles.clone();
        meter.sample(move |usage| {
            let [cpu, memory, gpu, video] = tiles.as_ref();
            cpu.set_text(&format!("CPU {}", percent(usage.cpu)));
            memory.set_text(&format!("Memory {}", bytes(usage.memory)));
            gpu.set_text(&format!("GPU {}", percent(usage.gpu)));
            video.set_text(&format!("Video memory {}", bytes(usage.video)));
        });
    });
    page
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.1} %"))
}

fn bytes(value: Option<u64>) -> String {
    value.map_or_else(
        || "—".to_owned(),
        |value| format!("{:.0} MB", value as f64 / 1_000_000.0),
    )
}
