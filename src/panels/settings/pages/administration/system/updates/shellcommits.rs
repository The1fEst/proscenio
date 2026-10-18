use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::administration::system::updates::updatepackages::{
    card, copy_button, fill, up_to_date_label,
};
use crate::panels::settings::pages::administration::system::updates::{
    add_update_button, go_back_on_install,
};
use crate::services::updates::{Commit, Job};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let updates = context.services.updates.clone();

    let empty_section = page.section("", "");
    empty_section.append(&up_to_date_label());
    let commit_section = page.section("commit", &tr("Commits"));
    let update_button = add_update_button(&page, &commit_section, &updates, Job::Shell);

    let show_commits = {
        let updates = updates.clone();
        let theme = context.theme.clone();
        move || {
            update_button.set_sensitive(updates.can_start(Job::Shell));
            let commits = updates.commits.borrow();
            fill(&commit_section, &commits, |commit| {
                commit_row(&theme, commit)
            });
            if let Some(section) = empty_section.parent() {
                section.set_visible(commits.is_empty());
            }
        }
    };
    show_commits();
    page.keep(updates.subscribe(show_commits));
    go_back_on_install(&page, context);
    page
}

pub fn commit_row(theme: &SharedTheme, commit: &Commit) -> gtk4::Box {
    let (card, lines) = card("commit", &commit.subject, false);
    let sha_label = text::styled_sized(commit.short_sha(), pixel_size::SMALLER);
    text::set_color(&sha_label, "colSubtext");
    sha_label.set_xalign(0.0);
    sha_label.set_selectable(true);
    lines.append(&Centered::filling_width(&sha_label));
    card.append(&copy_button(theme, &commit.sha));
    card
}
