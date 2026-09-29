use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{BASE_WIDTH, Context, Page};
use crate::services::sysinfo::{self, Disk};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
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
const DEFAULT_LINK_COLOUR: &str = "#2980b9";
const DOTFILES: &str = "https://github.com/The1fEst/dots-hyprland";
const UPSTREAM: &str = "https://github.com/end-4/dots-hyprland";

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
        &release.logo,
        &release.name,
        &[Line::Link(&release.home_url, pixel_size::NORMAL)],
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

    let dotfiles = page.section("folder_managed", &tr("Dotfiles"));
    dotfiles.append(&banner(
        "illogical-impulse",
        &tr("illogical-impulse"),
        &[
            Line::Link(DOTFILES, pixel_size::NORMAL),
            Line::Credit("Forked from %1", UPSTREAM),
        ],
    ));
    dotfiles.append(&links(
        &page,
        vec![
            Link {
                icon: "auto_stories",
                filled: true,
                label: "Documentation",
                url: "https://end-4.github.io/dots-hyprland-wiki/en/ii-qs/02usage/".to_owned(),
            },
            Link {
                icon: "adjust",
                filled: false,
                label: "Issues",
                url: format!("{DOTFILES}/issues"),
            },
            Link {
                icon: "forum",
                filled: true,
                label: "Discussions",
                url: format!("{UPSTREAM}/discussions"),
            },
            Link {
                icon: "favorite",
                filled: true,
                label: "Donate",
                url: "https://github.com/sponsors/end-4".to_owned(),
            },
        ],
    ));
    page
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

enum Line<'a> {
    Link(&'a str, i32),
    Credit(&'a str, &'a str),
}

fn banner(icon: &str, title: &str, lines: &[Line]) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, BANNER_SPACING);
    row.set_halign(gtk4::Align::Start);
    row.set_margin_top(BANNER_MARGIN);
    row.set_margin_bottom(BANNER_MARGIN);
    let image = gtk4::Image::from_icon_name(icon);
    image.set_pixel_size(BANNER_ICON);
    row.append(&image);
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, BANNER_LINES);
    column.set_valign(gtk4::Align::Center);
    let name = text::styled_sized(title, pixel_size::TITLE);
    name.set_xalign(0.0);
    column.append(&Centred::filling_width(&name));
    let colour = text::kdeglobals("Colors:View", "ForegroundLink")
        .unwrap_or_else(|| DEFAULT_LINK_COLOUR.to_owned());
    for line in lines {
        let (label, size) = match line {
            Line::Link(url, size) => (link_label(&link_markup(url, &colour)), *size),
            Line::Credit(template, url) => {
                let markup = glib::markup_escape_text(&tr(template))
                    .replace("%1", &link_markup(url, &colour));
                let label = link_label(&markup);
                text::set_color(&label, "colSubtext");
                (label, pixel_size::SMALLER)
            }
        };
        text::set_font(&label, text::Family::Main, size as f64, "wght=450");
        column.append(&Centred::filling_width(&label));
    }
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

fn link_markup(url: &str, colour: &str) -> String {
    let url = glib::markup_escape_text(url);
    format!("<a href=\"{url}\"><span foreground=\"{colour}\" underline=\"none\">{url}</span></a>")
}

fn open(url: &str) {
    let _ = gtk4::gio::AppInfo::launch_default_for_uri(url, gtk4::gio::AppLaunchContext::NONE);
}

fn link_label(markup: &str) -> gtk4::Label {
    let label = text::styled("");
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
