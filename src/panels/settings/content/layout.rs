use gtk4::prelude::*;
use std::rc::Rc;

use super::{GROUP_CLASS, Page, Parent, SECTION_SPACING};
use crate::core::tools::{self, Tool};
use crate::ui::theme::{pixel_size, rounding};
use crate::ui::widgets::display::loading::LoadingIndicator;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::tooltip::Tooltip;

const SECTION_HEADER_SPACING: i32 = 6;
const SECTION_CONTENT_SPACING: i32 = 4;
const SUBSECTION_TOP: i32 = 4;
const SUBSECTION_SPACING: i32 = 2;
const SUBSECTION_LABEL_START: i32 = 2;
const ROW_GAP: i32 = 4;
const LINK_HEIGHT: i32 = 56;
const LINK_PADDING: i32 = 12;
const LINK_SPACING: i32 = 12;
const NOTICE_SPACING: i32 = 8;
const BUSY_SIZE: i32 = 18;
const BUSY_MARGIN: i32 = 4;

/// The QML symbol loader gives its item a width and no height, so a row sizes to its
/// text and the symbol hangs across the middle of it.
fn strip_height(child: &impl IsA<gtk4::Widget>) -> gtk4::Widget {
    let width = child.measure(gtk4::Orientation::Horizontal, -1).1;
    let slot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    slot.set_size_request(width, 0);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&slot));
    child.set_valign(gtk4::Align::Center);
    overlay.add_overlay(child);
    overlay.set_valign(gtk4::Align::Center);
    overlay.upcast()
}

impl Page {
    pub fn section(&self, icon: &str, title: &str) -> gtk4::Box {
        Self::section_into(&self.column, icon, title)
    }

    pub fn sections() -> gtk4::Box {
        gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING)
    }

    pub fn section_into(column: &gtk4::Box, icon: &str, title: &str) -> gtk4::Box {
        let section = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_HEADER_SPACING);
        if !icon.is_empty() || !title.is_empty() {
            let header = gtk4::Box::new(gtk4::Orientation::Horizontal, SECTION_HEADER_SPACING);
            if !icon.is_empty() {
                let symbol = text::symbol(icon, pixel_size::HUGEASS as f64);
                text::set_color(&symbol, "colOnSecondaryContainer");
                header.append(&strip_height(&Centered::integral(&symbol)));
            }
            let title_label = text::styled(title);
            text::set_font(
                &title_label,
                text::Family::Main,
                pixel_size::LARGER as f64,
                "wght=450",
            );
            text::set_color(&title_label, "colOnSecondaryContainer");
            header.append(&Centered::new(&title_label));
            section.append(&header);
        }
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_CONTENT_SPACING);
        content.add_css_class(GROUP_CLASS);
        section.append(&content);
        column.append(&section);
        content
    }

    pub fn busy_section(&self, icon: &str, title: &str) -> (gtk4::Box, Rc<LoadingIndicator>) {
        let content = self.section(icon, title);
        let indicator = LoadingIndicator::new(&self.theme, BUSY_SIZE);
        indicator.area.set_margin_start(BUSY_MARGIN);
        indicator.area.set_visible(false);
        if let Some(header) = content.prev_sibling().and_downcast::<gtk4::Box>() {
            header.append(&indicator.area);
        }
        (content, indicator)
    }

    pub fn append(&self, child: &impl IsA<gtk4::Widget>) {
        self.column.append(child);
    }

    pub fn subsection(&self, parent: &impl Parent, title: &str, tip: &str) -> gtk4::Box {
        let (content, tip) = self.unkept_subsection(parent, title, tip);
        if let Some(tip) = tip {
            self.keep(tip);
        }
        content
    }

    pub fn unkept_subsection(
        &self,
        parent: &impl Parent,
        title: &str,
        tip: &str,
    ) -> (gtk4::Box, Option<Rc<Tooltip>>) {
        let subsection = gtk4::Box::new(gtk4::Orientation::Vertical, SUBSECTION_SPACING);
        subsection.set_margin_top(SUBSECTION_TOP);
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        if !title.is_empty() {
            let label = text::styled(title);
            text::set_color(&label, "colSubtext");
            label.set_margin_start(SUBSECTION_LABEL_START);
            header.append(&Centered::new(&label));
        }
        let tip = (!tip.is_empty()).then(|| {
            let info = text::symbol("info", pixel_size::LARGE as f64);
            text::set_color(&info, "colSubtext");
            info.set_cursor_from_name(Some("help"));
            header.append(&Centered::integral(&info));
            self.unkept_tip(&info, tip)
        });
        subsection.append(&header);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, SUBSECTION_SPACING);
        content.add_css_class(GROUP_CLASS);
        subsection.append(&content);
        parent.add(&subsection);
        (content, tip)
    }

    pub fn subsection_root(content: &gtk4::Box) -> gtk4::Widget {
        content
            .parent()
            .expect("a subsection's content sits in the subsection")
    }

    pub fn set_subsection_title(content: &gtk4::Box, title: &str) {
        let label = content
            .prev_sibling()
            .and_then(|header| header.first_child())
            .and_then(|centered| centered.first_child())
            .and_downcast::<gtk4::Label>();
        if let Some(label) = label {
            label.set_text(title);
        }
    }

    pub fn tools_notice(&self, parent: &impl Parent, tools: &[&Tool], effect: &str) -> bool {
        let missing_tools = tools::find_missing(tools);
        if missing_tools.is_empty() {
            return true;
        }
        self.notice(
            parent,
            "info",
            &tools::missing_message(&missing_tools, effect),
        );
        false
    }

    pub fn notice(&self, parent: &impl Parent, icon: &str, message: &str) -> gtk4::Box {
        let notice = gtk4::Box::new(gtk4::Orientation::Horizontal, NOTICE_SPACING);
        notice.add_css_class("settings-notice");
        let symbol = text::symbol(icon, pixel_size::HUGE as f64);
        text::set_color(&symbol, "colOnPrimaryContainer");
        let symbol = Centered::integral(&symbol);
        symbol.set_valign(gtk4::Align::Start);
        notice.append(&symbol);
        let label = text::styled(message);
        text::set_color(&label, "colOnPrimaryContainer");
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_wrap_mode(gtk4::pango::WrapMode::Word);
        label.set_hexpand(true);
        notice.append(&label);
        parent.add(&notice);
        notice
    }

    pub fn link_row(
        &self,
        parent: &impl Parent,
        icon: &str,
        title: &str,
        subtitle: &str,
        action: impl Fn() + 'static,
    ) -> gtk4::Label {
        let button = RippleButton::new(&self.theme);
        button.set_radius(rounding::SMALL as f64);
        button.set_look(Look {
            background: |theme| theme.colors.col_layer2,
            ..Look::default()
        });
        button.set_size_request(-1, LINK_HEIGHT);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, LINK_SPACING);
        if !icon.is_empty() {
            let symbol = text::symbol(icon, pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnSecondaryContainer");
            row.append(&strip_height(&Centered::integral(&symbol)));
        }
        let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        lines.set_hexpand(true);
        lines.set_valign(gtk4::Align::Center);
        let title_label = text::styled(title);
        text::set_color(&title_label, "colOnLayer2");
        title_label.set_xalign(0.0);
        title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centered::filling_width(&title_label));
        let detail = text::styled_sized(subtitle, pixel_size::SMALLER);
        text::set_color(&detail, "colSubtext");
        detail.set_xalign(0.0);
        detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        detail.set_visible(!subtitle.is_empty());
        lines.append(&Centered::filling_width(&detail));
        row.append(&lines);
        let chevron = text::symbol("chevron_right", pixel_size::LARGER as f64);
        text::set_color(&chevron, "colSubtext");
        row.append(&strip_height(&Centered::integral(&chevron)));
        button.set_content(&row, LINK_PADDING, 0);
        button.connect_clicked(move |_| action());
        parent.add(&button);
        detail
    }

    pub fn row(&self, parent: &impl Parent) -> Row {
        let row = Row::new(ROW_GAP);
        parent.add(&row);
        row
    }

    pub fn uniform_row(&self, parent: &impl Parent) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_GAP);
        row.set_homogeneous(true);
        parent.add(&row);
        row
    }
}
