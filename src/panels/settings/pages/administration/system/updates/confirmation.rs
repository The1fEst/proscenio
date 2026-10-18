use gtk4::prelude::*;
use std::collections::HashSet;
use std::rc::Rc;

use super::shellcommits::commit_row;
use super::updatepackages::package_row;
use crate::core::i18n::{tr, trn};
use crate::services::updates::packages::version_change;
use crate::services::updates::{Commit, Job, Review, Updates};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::flickable::follow_scroll_settings;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const DIALOG_WIDTH: f64 = 460.0;
const DIALOG_HEIGHT: f64 = 600.0;
const LIST_SPACING: i32 = 4;
const HEADER_START: i32 = 8;

pub fn build(
    theme: &SharedTheme,
    updates: &Updates,
    on_closed: impl Fn() + 'static,
) -> Option<Rc<WindowDialog>> {
    let review = updates.review.borrow().clone()?;
    let job = updates.job.get()?;
    let dialog = WindowDialog::new(theme, Some(DIALOG_HEIGHT));
    dialog.set_background_width(DIALOG_WIDTH);
    dialog
        .column
        .add(&windowdialog::title(&tr("Update")), Place::default());
    if !review.reboot_packages.is_empty() {
        let note = text::styled(&trn(
            "%1 package needs a reboot to take effect",
            "%1 packages need a reboot to take effect",
            review.reboot_packages.len() as u64,
        ));
        text::set_color(&note, "colError");
        note.set_wrap(true);
        note.set_xalign(0.0);
        dialog.column.add(&note, Place::wide());
    }
    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());

    let list = gtk4::Box::new(gtk4::Orientation::Vertical, LIST_SPACING);
    match job {
        Job::System => fill_packages(&list, theme, &review),
        Job::Shell => fill_commits(&list, theme, &updates.commits.borrow()),
    }
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_child(Some(&list));
    follow_scroll_settings(&scroll);
    dialog.column.add(
        &scroll,
        Place {
            left: -windowdialog::PADDING,
            right: -windowdialog::PADDING,
            fills_width: true,
            fills_height: true,
            ..Place::default()
        },
    );

    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());
    let (buttons, place) = windowdialog::button_row();
    buttons.append(&windowdialog::spacer());
    let cancel_button = windowdialog::button(theme, &tr("Cancel"));
    cancel_button.connect_clicked({
        let dialog = Rc::downgrade(&dialog);
        move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.dismiss();
            }
        }
    });
    buttons.append(&cancel_button);
    let update_text = match job {
        Job::System => trn(
            "Update %1 package",
            "Update %1 packages",
            review.count() as u64,
        ),
        Job::Shell => tr("Update"),
    };
    let update_button = windowdialog::button(theme, &update_text);
    update_button.connect_clicked({
        let updates = updates.clone();
        move |_| updates.confirm()
    });
    buttons.append(&update_button);
    dialog.column.add(&buttons, place);
    dialog.connect_closed(on_closed);
    Some(dialog)
}

fn fill_commits(list: &gtk4::Box, theme: &SharedTheme, commits: &[Commit]) {
    list.append(&build_section_header(&tr("Commits")));
    for commit in commits {
        list.append(&commit_row(theme, commit));
    }
}

fn fill_packages(list: &gtk4::Box, theme: &SharedTheme, review: &Review) {
    let reboot_packages: HashSet<&str> =
        review.reboot_packages.iter().map(String::as_str).collect();
    let needs_reboot = |name: &str| reboot_packages.contains(name);
    if !review.changes.is_empty() {
        list.append(&build_section_header(&tr("Repositories")));
    }
    for change in &review.changes {
        let to = if change.to.is_empty() {
            tr("removed")
        } else {
            change.to.clone()
        };
        list.append(&package_row(
            theme,
            &change.name,
            &change.from,
            &to,
            needs_reboot(&change.name),
        ));
    }
    if !review.aur_packages.is_empty() {
        list.append(&build_section_header(&tr("AUR")));
    }
    for line in &review.aur_packages {
        let (name, from, to) = version_change(line).unwrap_or((line, "", ""));
        list.append(&package_row(theme, name, from, to, needs_reboot(name)));
    }
}

fn build_section_header(title: &str) -> gtk4::Label {
    let header = windowdialog::section_header(title);
    header.set_margin_start(HEADER_START);
    header
}
