use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::services::sysinfo::{Resources, Usage};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::popup;
use crate::ui::widgets::ring::{Ring, Shape};
use crate::ui::widgets::text;

const VERTICAL_SPACING: i32 = 10;
const VERTICAL_RING: Shape = Shape {
    size: 18,
    icon_size: 13.0,
    icon_weight: 500,
};

struct Meter {
    ring: Ring,
    value: Option<gtk4::Label>,
    threshold: f64,
}

impl Meter {
    fn vertical(theme: &SharedTheme, icon: &'static str, threshold: f64) -> (Self, gtk4::Widget) {
        let ring = Ring::shaped(theme.clone(), icon, VERTICAL_RING);
        ring.area.set_halign(gtk4::Align::Center);
        let area = ring.area.clone().upcast();
        (
            Meter {
                ring,
                value: None,
                threshold,
            },
            area,
        )
    }

    fn new(theme: &SharedTheme, icon: &'static str, threshold: f64) -> (Self, gtk4::Box) {
        let ring = Ring::new(theme.clone(), icon);
        let value = text::styled("0");
        text::set_color(&value, "colOnLayer1");
        value.set_size_request(full_value_width(), -1);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        row.append(&ring.area);
        row.append(&Centred::optical(&value));

        (
            Meter {
                ring,
                value: Some(value),
                threshold,
            },
            row,
        )
    }

    fn set(&self, fraction: f64) {
        let percent = fraction * 100.0;
        self.ring.set(fraction, percent >= self.threshold);
        if let Some(value) = &self.value {
            value.set_text(&format!("{}", percent.round() as i64));
        }
    }
}

fn full_value_width() -> i32 {
    let (family, weight) = text::application_font();
    let probe = gtk4::Label::new(Some("100"));
    let mut font = gtk4::pango::FontDescription::new();
    font.set_family(&family);
    font.set_absolute_size(15.0 * gtk4::pango::SCALE as f64);
    font.set_variations(Some(&format!("wght={weight},opsz=18")));
    let attributes = gtk4::pango::AttrList::new();
    attributes.insert(gtk4::pango::AttrFontDesc::new(&font));
    probe.set_attributes(Some(&attributes));
    probe.measure(gtk4::Orientation::Horizontal, -1).1
}

pub fn build(
    theme: &SharedTheme,
    config: &Config,
    resources: &Rc<Resources>,
    scope: &Scope,
) -> gtk4::Widget {
    let (area, meters): (gtk4::Widget, _) = if config.vertical {
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, VERTICAL_SPACING);
        let (memory, memory_ring) = Meter::vertical(theme, "memory", config.memory_warning);
        let (swap, swap_ring) = Meter::vertical(theme, "swap_horiz", config.swap_warning);
        let (cpu, cpu_ring) = Meter::vertical(theme, "planner_review", config.cpu_warning);
        column.append(&memory_ring);
        column.append(&swap_ring);
        column.append(&cpu_ring);
        (column.upcast(), (memory, swap, cpu))
    } else {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.set_valign(gtk4::Align::Center);
        row.set_halign(gtk4::Align::Center);
        let area = Centred::new(&row);
        area.set_hexpand(true);
        area.set_size_request(-1, crate::core::config::BASE_BAR_HEIGHT);

        let (memory, memory_row) = Meter::new(theme, "memory", config.memory_warning);
        let (swap, swap_row) = Meter::new(theme, "swap_horiz", config.swap_warning);
        let (cpu, cpu_row) = Meter::new(theme, "planner_review", config.cpu_warning);
        swap_row.set_margin_start(6);
        cpu_row.set_margin_start(6);
        row.append(&memory_row);
        row.append(&swap_row);
        row.append(&cpu_row);
        (area.upcast(), (memory, swap, cpu))
    };

    let columns = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    let (memory_column, memory_values) = meter_column("memory", "RAM");
    let (swap_column, swap_values) = meter_column("swap_horiz", "Swap");
    columns.append(&memory_column);
    columns.append(&swap_column);
    let cpu_column = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    cpu_column.set_valign(gtk4::Align::Start);
    cpu_column.append(&popup::header("planner_review", "CPU"));
    let cpu_rows = popup::column();
    let (load_row, load) = popup::value("bolt", "Load:", "0%");
    load_row.set_halign(gtk4::Align::Start);
    cpu_rows.append(&load_row);
    cpu_column.append(&cpu_rows);
    columns.append(&cpu_column);

    let show = {
        let resources = resources.clone();
        move || {
            let usage: Usage = resources.usage();
            meters.0.set(usage.memory);
            meters.1.set(usage.swap);
            meters.2.set(usage.cpu);
            for (label, kilobytes) in memory_values.iter().zip([
                usage.memory_used(),
                usage.memory_free,
                usage.memory_total,
            ]) {
                label.set_text(&gigabytes(kilobytes));
            }
            swap_column.set_visible(usage.swap_total > 0.0);
            for (label, kilobytes) in
                swap_values
                    .iter()
                    .zip([usage.swap_used(), usage.swap_free, usage.swap_total])
            {
                label.set_text(&gigabytes(kilobytes));
            }
            load.set_text(&format!("{}%", (usage.cpu * 100.0).round()));
        }
    };
    show();
    scope.keep(resources.subscribe(show));

    popup::attach(&area, popup::Bar::of(config), &columns);

    area
}

fn meter_column(icon: &str, name: &str) -> (gtk4::Box, [gtk4::Label; 3]) {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    column.set_valign(gtk4::Align::Start);
    column.append(&popup::header(icon, name));

    let rows = popup::column();
    let (used_row, used) = popup::value("clock_loader_60", "Used:", "0");
    let (free_row, free) = popup::value("check_circle", "Free:", "0");
    let (total_row, total) = popup::value("empty_dashboard", "Total:", "0");
    for row in [&used_row, &free_row, &total_row] {
        row.set_halign(gtk4::Align::Start);
        rows.append(row);
    }
    column.append(&rows);

    (column, [used, free, total])
}

fn gigabytes(kilobytes: f64) -> String {
    format!("{:.1} GB", kilobytes / (1024.0 * 1024.0))
}
