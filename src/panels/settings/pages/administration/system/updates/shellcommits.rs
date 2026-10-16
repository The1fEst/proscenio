use gtk4::prelude::*;
use std::rc::Rc;

use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::administration::system::updates::updatepackages::{
    card, fill, up_to_date_label,
};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let updates = context.services.updates.clone();

    let empty_section = page.section("", "");
    empty_section.append(&up_to_date_label());
    let commit_section = page.section("", "");

    let show_commits = {
        let updates = updates.clone();
        move || {
            let subjects = updates.commit_subjects.borrow();
            fill(&commit_section, &subjects, |subject| commit_row(subject));
            if let Some(section) = empty_section.parent() {
                section.set_visible(subjects.is_empty());
            }
        }
    };
    show_commits();
    page.keep(updates.subscribe(show_commits));
    page
}

fn commit_row(subject: &str) -> gtk4::Box {
    card("commit", subject, false).0
}
