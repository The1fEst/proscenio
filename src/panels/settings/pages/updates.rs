use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{Context, Page};
use crate::services::updates::{Job, Updates};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::text::{self, Family};

const LIST_LINES: usize = 8;
const LOG_HEIGHT: i32 = 320;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let updates = context.services.updates.clone();

    let check = page.section("", "");
    let (refresh, _) = page.icon_button("refresh", false, &tr("Check now"), {
        let updates = updates.clone();
        move || updates.refresh()
    });
    check.append(&refresh);

    let system = page.section("deployed_code_update", &tr("System"));
    let system_status = text::styled("");
    system_status.set_xalign(0.0);
    system.append(&system_status);
    let packages = list(&system);
    let (system_update, _) = page.icon_button("upgrade", true, &tr("Update"), {
        let updates = updates.clone();
        move || updates.start(Job::System)
    });
    system.append(&system_update);

    let shell = page.section("upgrade", &tr("Shell"));
    let shell_status = text::styled("");
    shell_status.set_xalign(0.0);
    shell.append(&shell_status);
    let commits = list(&shell);
    let (shell_update, _) = page.icon_button("upgrade", true, &tr("Update"), {
        let updates = updates.clone();
        move || updates.start(Job::Shell)
    });
    shell.append(&shell_update);

    let (output, busy) = page.busy_section("terminal", &tr("Log"));
    let outcome = text::styled("");
    outcome.set_xalign(0.0);
    output.append(&outcome);
    let log = gtk4::TextView::new();
    log.set_editable(false);
    log.set_cursor_visible(false);
    log.set_wrap_mode(gtk4::WrapMode::WordChar);
    log.add_css_class("settings-log");
    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk4::PolicyType::Never);
    scroller.set_size_request(-1, LOG_HEIGHT);
    scroller.set_child(Some(&log));
    stick_to_bottom(&scroller);
    output.append(&scroller);

    let follow = {
        let updates = updates.clone();
        let output = output.clone();
        move || {
            let count = updates.count();
            system_status.set_text(&if count > 0 {
                trf("%1 packages", &[&count.to_string()])
            } else {
                tr("Up to date")
            });
            show_list(&packages, &updates.packages.borrow());
            let behind = updates.behind.get();
            shell_status.set_text(&if behind > 0 {
                trf("%1 commits behind", &[&behind.to_string()])
            } else {
                tr("Up to date")
            });
            show_list(&commits, &updates.commits.borrow());

            let job = updates.job.get();
            system_update.set_sensitive(job.is_none() && updates.available.get());
            shell_update.set_sensitive(job.is_none());
            busy.area.set_visible(job.is_some());
            busy.set_loading(job.is_some());
            outcome.set_text(&match (job, updates.succeeded.get()) {
                (Some(_), _) => tr("Updating…"),
                (None, Some(true)) => tr("Done"),
                (None, Some(false)) => tr("Failed"),
                (None, None) => String::new(),
            });
            if let Some(section) = output.parent() {
                section.set_visible(job.is_some() || updates.succeeded.get().is_some());
            }
        }
    };
    follow();
    page.keep(updates.subscribe(follow.clone()));
    page.watch("/updates", follow);

    let append = follow_log(&updates, &log);
    append();
    page.keep(updates.subscribe_output(append));
    page
}

fn list(parent: &gtk4::Box) -> gtk4::Label {
    let label = gtk4::Label::new(None);
    text::set_font(&label, Family::Monospace, pixel_size::SMALLER as f64, "");
    text::set_color(&label, "colSubtext");
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    label.set_selectable(true);
    parent.append(&label);
    label
}

fn show_list(label: &gtk4::Label, lines: &[String]) {
    label.set_text(&shortened(lines, LIST_LINES));
    label.set_visible(!lines.is_empty());
}

fn shortened(lines: &[String], limit: usize) -> String {
    if lines.len() <= limit {
        return lines.join("\n");
    }
    let shown = &lines[..limit - 1];
    format!("{}\n+{}", shown.join("\n"), lines.len() - shown.len())
}

fn stick_to_bottom(scroller: &gtk4::ScrolledWindow) {
    let adjustment = scroller.vadjustment();
    let stuck = Rc::new(Cell::new(true));
    adjustment.connect_value_changed({
        let stuck = stuck.clone();
        move |adjustment| {
            stuck.set(adjustment.value() + adjustment.page_size() >= adjustment.upper() - 1.0);
        }
    });
    adjustment.connect_changed(move |adjustment| {
        if stuck.get() {
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
        }
    });
}

fn follow_log(updates: &Updates, view: &gtk4::TextView) -> impl Fn() + Clone + use<> {
    let buffer = view.buffer();
    let shown = Rc::new(Cell::new(0usize));
    let updates = updates.clone();
    move || {
        let log = updates.log.borrow();
        if log.len() < shown.get() {
            buffer.set_text("");
            shown.set(0);
        }
        buffer.insert(&mut buffer.end_iter(), &log[shown.get()..]);
        shown.set(log.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_list_shows_its_first_lines_and_how_many_more_there_are() {
        let lines = |count: usize| -> Vec<String> { (1..=count).map(|n| n.to_string()).collect() };
        assert_eq!(shortened(&lines(3), 3), "1\n2\n3");
        assert_eq!(shortened(&lines(5), 3), "1\n2\n+3");
        assert_eq!(shortened(&[], 3), "");
    }
}
