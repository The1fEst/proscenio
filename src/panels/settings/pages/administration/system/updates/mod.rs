mod confirmation;
pub mod shellcommits;
pub mod updatepackages;

use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::i18n::{tr, trf, trn};
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::administration::system::about;
use crate::services::updates::updateprogress::Step;
use crate::services::updates::{Job, Notice, Updates};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::progress::{Colors, ProgressBar};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::overlay::windowdialog::WindowDialog;

const LOG_HEIGHT: i32 = 320;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let updates = context.services.updates.clone();
    let present: Rc<dyn Fn(Rc<WindowDialog>)> = Rc::new(context.dialog_presenter());
    let review_dialog: Rc<RefCell<Option<Rc<WindowDialog>>>> = Rc::default();

    let (system_section, system_check_indicator) =
        page.busy_section("deployed_code_update", &tr("System"));
    let (system_check_button, system_update_button) =
        header_actions(&page, &system_section, &updates, Job::System);
    let (packages_button, packages_summary_label) = page.link_row_with_button(
        &system_section,
        "deployed_code",
        &tr("Packages"),
        " ",
        context.subpage_opener("updatepackages"),
    );

    let (shell_section, shell_check_indicator) = page.busy_section("", &tr("Shell"));
    if let Some(header) = shell_section.prev_sibling().and_downcast::<gtk4::Box>() {
        let logo = about::shell_logo(pixel_size::HUGEASS);
        logo.set_valign(gtk4::Align::Center);
        header.prepend(&logo);
    }
    let (shell_check_button, shell_update_button) =
        header_actions(&page, &shell_section, &updates, Job::Shell);
    let (commits_button, commits_summary_label) = page.link_row_with_button(
        &shell_section,
        "commit",
        &tr("Commits"),
        " ",
        context.subpage_opener("shellcommits"),
    );

    let (output_section, loading_indicator) = page.busy_section("downloading", &tr("Update"));
    let outcome_label = text::styled("");
    outcome_label.set_xalign(0.0);
    output_section.append(&outcome_label);
    let progress_bar = ProgressBar::new();
    progress_bar.set_hexpand(true);
    {
        let theme = context.theme.borrow();
        progress_bar.set_colors(Colors {
            highlight: theme.colors.col_primary,
            track: theme.m3.secondary_container,
        });
    }
    output_section.append(&progress_bar);
    let step_label = text::styled_sized("", pixel_size::SMALLER);
    text::set_color(&step_label, "colSubtext");
    step_label.set_xalign(0.0);
    step_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    output_section.append(&step_label);
    let revealer = gtk4::Revealer::new();
    let (log_button, _) = page.icon_button("terminal", false, &tr("Log"), {
        let revealer = revealer.clone();
        move || revealer.set_reveal_child(!revealer.reveals_child())
    });
    output_section.append(&log_button);
    let log_view = gtk4::TextView::new();
    log_view.set_editable(false);
    log_view.set_cursor_visible(false);
    log_view.set_wrap_mode(gtk4::WrapMode::WordChar);
    log_view.add_css_class("settings-log");
    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk4::PolicyType::Never);
    scroller.set_size_request(-1, LOG_HEIGHT);
    scroller.set_child(Some(&log_view));
    stick_to_bottom(&scroller);
    revealer.set_child(Some(&scroller));
    output_section.append(&revealer);

    let show_state = {
        let updates = updates.clone();
        let output_section = output_section.clone();
        let theme = context.theme.clone();
        move || {
            if updates.review.borrow().is_none() {
                if let Some(dialog) = review_dialog.take() {
                    dialog.dismiss();
                }
            } else if review_dialog.borrow().is_none() {
                let on_closed = {
                    let review_dialog = review_dialog.clone();
                    let updates = updates.clone();
                    move || {
                        if review_dialog.take().is_some() {
                            updates.cancel();
                        }
                    }
                };
                if let Some(dialog) = confirmation::build(&theme, &updates, on_closed) {
                    review_dialog.replace(Some(dialog.clone()));
                    present(dialog);
                }
            }
            let count = updates.count();
            let packages_text = trn("%1 package", "%1 packages", count as u64);
            packages_summary_label.set_text(&match (
                count > 0,
                updates.has_package_check_failed.get(),
            ) {
                (true, false) => {
                    format!("{packages_text} · {}", updatepackages::summary(&updates))
                }
                (true, true) => {
                    format!("{packages_text} · {}", tr("Could not check for updates"))
                }
                (false, true) => tr("Could not check for updates"),
                (false, false) => tr("Up to date"),
            });
            packages_button.set_sensitive(count > 0);
            let commits_behind = updates.commits_behind.get();
            commits_button.set_sensitive(commits_behind > 0);
            commits_summary_label.set_text(&if commits_behind > 0 {
                trn(
                    "%1 commit behind",
                    "%1 commits behind",
                    commits_behind as u64,
                )
            } else {
                tr("Up to date")
            });

            let job = updates.job.get();
            let is_checking_packages = updates.is_refreshing_packages.get();
            system_check_indicator
                .area
                .set_visible(is_checking_packages);
            system_check_indicator.set_loading(is_checking_packages);
            system_check_button.set_sensitive(!is_checking_packages);
            let is_checking_shell = updates.is_refreshing_shell.get();
            shell_check_indicator.area.set_visible(is_checking_shell);
            shell_check_indicator.set_loading(is_checking_shell);
            shell_check_button.set_sensitive(!is_checking_shell);
            system_check_button.set_visible(count == 0);
            system_update_button.set_visible(count > 0);
            system_update_button.set_sensitive(job.is_none() && updates.has_package_manager.get());
            shell_check_button.set_visible(commits_behind == 0);
            shell_update_button.set_visible(commits_behind > 0);
            shell_update_button.set_sensitive(job.is_none());
            loading_indicator.area.set_visible(job.is_some());
            loading_indicator.set_loading(job.is_some());
            let notice = updates.notice.get();
            outcome_label.set_text(&match (job, updates.last_job_succeeded.get(), notice) {
                (Some(_), _, _) if updates.is_installing.get() => tr("Updating…"),
                (Some(_), _, _) => tr("Checking for updates…"),
                (None, Some(true), _) => tr("Done"),
                (None, Some(false), _) => tr("Failed"),
                (None, None, Some(Notice::UpToDate)) => tr("Up to date"),
                (None, None, Some(Notice::CheckFailed)) => tr("Could not check for updates"),
                (None, None, None) => String::new(),
            });
            if let Some(section) = output_section.parent() {
                section.set_visible(
                    job.is_some() || updates.last_job_succeeded.get().is_some() || notice.is_some(),
                );
            }
        }
    };
    show_state();
    page.keep(updates.subscribe(show_state.clone()));
    page.watch("/updates", show_state);

    let append_log = follow_log(&updates, &log_view);
    let show_output = {
        let updates = updates.clone();
        move || {
            append_log();
            let mut progress = updates.progress.borrow_mut();
            progress_bar.set_visible(progress.is_some());
            step_label.set_visible(progress.is_some() && updates.job.get().is_some());
            let Some(progress) = progress.as_mut() else {
                return;
            };
            progress_bar.set_value(progress.fraction());
            let is_running = updates.job.get().is_some();
            progress_bar.set_wavy(is_running);
            progress_bar.set_wave_moving(is_running);
            step_label.set_text(&step_text(progress.step()));
        }
    };
    show_output();
    page.keep(updates.subscribe_output(show_output.clone()));
    page.keep(updates.subscribe(show_output));
    page
}

fn step_text(step: &Step) -> String {
    match step {
        Step::Checking => tr("Checking for updates…"),
        Step::Downloading {
            received_bytes,
            total_bytes,
        } => trf(
            "Downloading %1 of %2",
            &[
                &glib::format_size(*received_bytes),
                &glib::format_size(*total_bytes),
            ],
        ),
        Step::Installing { index, count, name } => trf(
            "Installing %1 of %2: %3",
            &[&index.to_string(), &count.to_string(), name],
        ),
        Step::Hooks { index, count } => trf(
            "Running hooks: %1 of %2",
            &[&index.to_string(), &count.to_string()],
        ),
        Step::Building {
            name,
            built_count,
            count,
        } => trf(
            "Building %1 (%2 of %3)",
            &[name, &(built_count + 1).to_string(), &count.to_string()],
        ),
    }
}

fn header_actions(
    page: &Page,
    section: &gtk4::Box,
    updates: &Updates,
    job: Job,
) -> (RippleButton, RippleButton) {
    let (check_button, _) = page.icon_button("refresh", false, &tr("Check now"), {
        let updates = updates.clone();
        move || updates.check(job)
    });
    let (update_button, _) = page.icon_button("upgrade", true, &tr("Update"), {
        let updates = updates.clone();
        move || updates.start(job)
    });
    if let Some(header) = section.prev_sibling().and_downcast::<gtk4::Box>() {
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        header.append(&check_button);
        header.append(&update_button);
    }
    (check_button, update_button)
}

fn stick_to_bottom(scroller: &gtk4::ScrolledWindow) {
    let adjustment = scroller.vadjustment();
    let is_at_bottom = Rc::new(Cell::new(true));
    adjustment.connect_value_changed({
        let is_at_bottom = is_at_bottom.clone();
        move |adjustment| {
            is_at_bottom
                .set(adjustment.value() + adjustment.page_size() >= adjustment.upper() - 1.0);
        }
    });
    adjustment.connect_changed(move |adjustment| {
        if is_at_bottom.get() {
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
        }
    });
}

fn follow_log(updates: &Updates, view: &gtk4::TextView) -> impl Fn() + Clone + use<> {
    let buffer = view.buffer();
    let copied_length = Rc::new(Cell::new(0usize));
    let updates = updates.clone();
    move || {
        let log = updates.log.borrow();
        if log.len() < copied_length.get() {
            buffer.set_text("");
            copied_length.set(0);
        }
        buffer.insert(&mut buffer.end_iter(), &log[copied_length.get()..]);
        copied_length.set(log.len());
    }
}
