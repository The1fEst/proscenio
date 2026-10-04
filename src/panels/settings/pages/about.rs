use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::assets;
use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{BASE_WIDTH, Context, Page};
use crate::services::sysinfo::{self, Disk};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::customicon;
use crate::ui::widgets::flow::Flow;
use crate::ui::widgets::progress::{Colours, ProgressBar};
use crate::ui::widgets::text;

const FACT_MARGIN: i32 = 8;
const FACT_LABEL_WIDTH: i32 = 140;
const FACT_SPACING: i32 = 4;
const DISK_COLUMN_WIDTH: i32 = 290;
const DISK_SPACING: i32 = 8;
const DISK_ROWS: i32 = 4;
const DISK_HEADER_SPACING: i32 = 6;
const DISK_BAR_MARGIN: i32 = 2;
const BANNER_SPACING: i32 = 20;
const BANNER_MARGIN: i32 = 10;
const BANNER_ICON: i32 = 80;
const BANNER_LINES: i32 = 5;
const LINKS_SPACING: i32 = 5;
const REPOSITORY: &str = "https://github.com/The1fEst/proscenio";
const DOCUMENTATION: &str = "https://the1fest.github.io/proscenio/";
const VERSION: &str = env!("PROSCENIO_VERSION");
const LOGO_COLORS: [&str; 4] = [
    "m3onPrimaryFixedVariant",
    "m3primaryFixedDim",
    "m3onPrimaryFixed",
    "m3primaryFixed",
];

struct Link {
    icon: &'static str,
    filled: bool,
    label: &'static str,
    url: String,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let device = page.section("memory", &tr("Device"));
    let machine = sysinfo::machine();
    fact(&device, &tr("Name"), &machine.hostname);
    fact(&device, &tr("Processor"), &machine.processor);
    let graphics = fact(&device, &tr("Graphics"), "");
    fact(&device, &tr("Memory"), &machine.memory);
    fact(&device, &tr("Kernel"), &machine.kernel);
    fact(&device, &tr("Session"), &machine.session);
    let graphics = graphics.downgrade();
    sysinfo::graphics(move |found| {
        if let Some(graphics) = graphics.upgrade() {
            show_fact(&graphics, &found);
        }
    });

    let storage = page.section("storage", &tr("Storage"));
    if let Some(section) = storage.parent() {
        section.set_visible(false);
    }
    let grid = gtk4::Grid::new();
    grid.set_row_spacing(DISK_SPACING as u32);
    grid.set_column_spacing(DISK_SPACING as u32);
    grid.set_column_homogeneous(true);
    storage.append(&grid);
    let (grid, theme) = (grid.downgrade(), context.theme.clone());
    sysinfo::disks(move |disks| {
        let Some(grid) = grid.upgrade() else {
            return;
        };
        let columns = (BASE_WIDTH / DISK_COLUMN_WIDTH).max(1);
        for (index, disk) in disks.iter().enumerate() {
            let index = index as i32;
            grid.attach(
                &disk_card(&theme, disk),
                index % columns,
                index / columns,
                1,
                1,
            );
        }
        if let Some(section) = grid.parent().and_then(|content| content.parent()) {
            section.set_visible(!disks.is_empty());
        }
    });

    let release = sysinfo::os_release();
    let distro = page.section("box", &tr("Distro"));
    distro.append(&banner(
        &logo(&release.logo),
        &release.name,
        &release.home_url,
    ));
    distro.append(&links(
        &page,
        vec![
            Link {
                icon: "auto_stories",
                filled: true,
                label: "Documentation",
                url: release.documentation_url.clone(),
            },
            Link {
                icon: "support",
                filled: true,
                label: "Help & Support",
                url: release.support_url.clone(),
            },
            Link {
                icon: "bug_report",
                filled: true,
                label: "Report a Bug",
                url: release.bug_report_url.clone(),
            },
            Link {
                icon: "policy",
                filled: false,
                label: "Privacy Policy",
                url: release.privacy_policy_url.clone(),
            },
        ],
    ));

    let shell = page.section("curtains", &tr("Shell"));
    shell.append(&banner(&shell_logo(), "proscenio", REPOSITORY));
    fact(&shell, &tr("Version"), VERSION);
    fact(
        &shell,
        "GTK",
        &format!(
            "{}.{}.{}",
            gtk4::major_version(),
            gtk4::minor_version(),
            gtk4::micro_version()
        ),
    );
    let renderer = fact(&shell, &tr("Renderer"), "").downgrade();
    shell.connect_map(move |shell| {
        let name = shell
            .native()
            .and_then(|native| native.renderer())
            .map(|renderer| renderer_name(renderer.type_().name()));
        if let (Some(row), Some(name)) = (renderer.upgrade(), name) {
            show_fact(&row, name);
        }
    });
    shell.append(&links(
        &page,
        vec![
            Link {
                icon: "auto_stories",
                filled: true,
                label: "Documentation",
                url: DOCUMENTATION.to_owned(),
            },
            Link {
                icon: "adjust",
                filled: false,
                label: "Issues",
                url: format!("{REPOSITORY}/issues"),
            },
        ],
    ));
    page
}

fn shell_logo() -> gtk4::Widget {
    let mut lines = assets::LOGO.lines();
    let header = lines.next().unwrap_or_default();
    let paths = lines
        .map(str::trim)
        .filter(|line| line.starts_with("<path"));
    let overlay = gtk4::Overlay::new();
    for (path, color) in paths.zip(LOGO_COLORS) {
        let layer = customicon::from_svg(format!("{header}{path}</svg>"), BANNER_ICON);
        text::set_color(&layer, color);
        if overlay.child().is_none() {
            overlay.set_child(Some(&layer));
        } else {
            overlay.add_overlay(&layer);
        }
    }
    overlay.upcast()
}

fn renderer_name(type_name: &str) -> &str {
    match type_name {
        "GskCairoRenderer" => "Cairo",
        "GskGLRenderer" | "GskNglRenderer" => "OpenGL",
        "GskVulkanRenderer" => "Vulkan",
        other => other,
    }
}

fn fact(parent: &gtk4::Box, label: &str, value: &str) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, FACT_SPACING);
    let name = text::styled(label);
    text::set_color(&name, "colSubtext");
    name.set_xalign(0.0);
    let name_box = Centred::filling_width(&name);
    name_box.set_margin_start(FACT_MARGIN);
    name_box.set_size_request(FACT_LABEL_WIDTH, -1);
    row.append(&name_box);
    let shown = text::styled(value);
    text::set_color(&shown, "colOnLayer0");
    shown.set_xalign(0.0);
    shown.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let shown_box = Centred::filling_width(&shown);
    shown_box.set_hexpand(true);
    shown_box.set_margin_end(FACT_MARGIN);
    row.append(&shown_box);
    row.set_visible(!value.is_empty());
    parent.append(&row);
    row
}

fn show_fact(row: &gtk4::Box, value: &str) {
    let label = row
        .last_child()
        .and_then(|holder| holder.first_child())
        .and_downcast::<gtk4::Label>();
    if let Some(label) = label {
        label.set_text(value);
    }
    row.set_visible(!value.is_empty());
}

fn disk_card(theme: &SharedTheme, disk: &Disk) -> gtk4::Widget {
    let fraction = if disk.size == 0 {
        0.0
    } else {
        disk.used as f64 / disk.size as f64
    };
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, DISK_ROWS);
    column.add_css_class("settings-disk");
    column.set_hexpand(true);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, DISK_HEADER_SPACING);
    let symbol = text::symbol("hard_drive", pixel_size::HUGEASS as f64);
    text::set_color(&symbol, "colOnLayer1");
    header.append(&Centred::integral(&symbol));
    let mount = text::styled(&disk.mount);
    text::set_color(&mount, "colOnLayer1");
    mount.set_xalign(0.0);
    mount.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    let mount_box = Centred::filling_width(&mount);
    mount_box.set_hexpand(true);
    header.append(&mount_box);
    let percent = text::styled_sized(
        &format!("{}%", (fraction * 100.0).round()),
        pixel_size::SMALLER,
    );
    text::set_color(&percent, "colSubtext");
    header.append(&Centred::new(&percent));
    column.append(&header);

    let bar = ProgressBar::new();
    bar.set_hexpand(true);
    bar.set_margin_top(DISK_BAR_MARGIN);
    bar.set_margin_bottom(DISK_BAR_MARGIN);
    {
        let theme = theme.borrow();
        bar.set_colours(Colours {
            highlight: theme.colors.col_primary,
            track: theme.m3.secondary_container,
        });
    }
    bar.set_value(fraction);
    column.append(&bar);

    let free = text::styled_sized(
        &trf(
            "%1 free of %2",
            &[
                &sysinfo::human_size((disk.size - disk.used.min(disk.size)) as f64),
                &sysinfo::human_size(disk.size as f64),
            ],
        ),
        pixel_size::SMALLER,
    );
    text::set_color(&free, "colSubtext");
    free.set_xalign(0.0);
    column.append(&Centred::filling_width(&free));
    let source = text::styled_sized(
        &format!("{} · {}", disk.source, disk.fstype),
        pixel_size::SMALLEST,
    );
    text::set_color(&source, "colSubtext");
    source.set_xalign(0.0);
    source.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    column.append(&Centred::filling_width(&source));
    column.upcast()
}

fn logo(icon: &str) -> gtk4::Widget {
    let image = gtk4::Image::from_icon_name(icon);
    image.set_pixel_size(BANNER_ICON);
    image.upcast()
}

fn banner(image: &gtk4::Widget, title: &str, url: &str) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, BANNER_SPACING);
    row.set_halign(gtk4::Align::Start);
    row.set_margin_top(BANNER_MARGIN);
    row.set_margin_bottom(BANNER_MARGIN);
    image.set_size_request(BANNER_ICON, BANNER_ICON);
    row.append(image);
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, BANNER_LINES);
    column.set_valign(gtk4::Align::Center);
    let name = text::styled_sized(title, pixel_size::TITLE);
    name.set_xalign(0.0);
    column.append(&Centred::filling_width(&name));
    let link = link_label(&link_markup(url));
    text::set_font(
        &link,
        text::Family::Main,
        pixel_size::NORMAL as f64,
        "wght=450",
    );
    column.append(&Centred::filling_width(&link));
    row.append(&column);
    let mut widest: f64 = 0.0;
    let mut line = column.first_child();
    while let Some(holder) = line {
        if let Some(label) = holder.first_child().and_downcast::<gtk4::Label>() {
            let (_, logical) = label.layout().extents();
            widest = widest.max(logical.width() as f64 / gtk4::pango::SCALE as f64);
        }
        line = holder.next_sibling();
    }
    let width = (BANNER_ICON + BANNER_SPACING) as f64 + widest;
    row.set_margin_start(((BASE_WIDTH as f64 - width) / 2.0).round() as i32);
    row
}

fn link_markup(url: &str) -> String {
    let url = glib::markup_escape_text(url);
    format!("<a href=\"{url}\"><span underline=\"none\">{url}</span></a>")
}

fn open(url: &str) {
    let _ = gtk4::gio::AppInfo::launch_default_for_uri(url, gtk4::gio::AppLaunchContext::NONE);
}

fn link_label(markup: &str) -> gtk4::Label {
    let label = text::styled("");
    label.add_css_class("settings-link");
    label.set_markup(markup);
    label.set_xalign(0.0);
    label.connect_activate_link(|_, url| {
        open(url);
        glib::Propagation::Stop
    });
    label
}

fn links(page: &Page, links: Vec<Link>) -> Flow {
    let flow = Flow::new(LINKS_SPACING);
    flow.set_hexpand(true);
    for link in links {
        let url = link.url;
        let (button, _) =
            page.icon_button(link.icon, link.filled, &tr(link.label), move || open(&url));
        flow.append(&button);
    }
    flow
}
