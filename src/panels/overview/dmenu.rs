use gtk4::prelude::*;
use std::rc::Rc;

use super::launcher::Run;
use super::{
    LIST_MARGIN, LIST_MAX, LIST_SPACING, Overview, SEARCH_WIDTH_COLLAPSED, search_placeholder,
};
use crate::core::dmenu::{Reply, Request, Responder};

impl Overview {
    pub fn open_dmenu(self: &Rc<Self>, request: Request, responder: Responder) {
        if self.window.is_visible() {
            self.hide();
        }
        self.prompt.set_text(&request.prompt);
        self.prompt.set_visible(!request.prompt.is_empty());
        let placeholder = (!request.placeholder.is_empty()).then_some(request.placeholder.as_str());
        self.entry.set_placeholder_text(placeholder);
        self.dmenu_lines.set(request.lines);
        self.is_dmenu_prompt_only.set(request.is_prompt_only);
        self.is_opened_with_prefix.set(false);
        self.should_animate_width.set(false);
        self.entry.set_text("");
        self.launcher.set_query("");
        let session = self.launcher.begin_dmenu(request, responder);
        self.dmenu_session.set(session);
        self.window.set_visible(true);
        self.focus_entry();
        self.hold_grab();
        self.launcher.announce();
    }

    pub(super) fn leave_dmenu(&self) {
        let session = self.dmenu_session.replace(0);
        if session == 0 {
            return;
        }
        self.launcher.finish_dmenu(session, Reply::Cancellation);
        self.prompt.set_visible(false);
        self.entry.set_placeholder_text(Some(&search_placeholder()));
        self.entry.set_text("");
        self.dmenu_lines.set(0);
        self.is_dmenu_prompt_only.set(false);
        self.is_list_shown.set(false);
        self.rule.set_visible(false);
        self.scroller.set_visible(false);
        let (mut width, mut height) = (self.width.get(), self.height.get());
        width.jump(SEARCH_WIDTH_COLLAPSED);
        height.jump(0.0);
        self.width.set(width);
        self.height.set(height);
        self.apply(0);
    }

    pub(super) fn choose(&self, run: &Run) {
        if matches!(run, Run::Dmenu(..)) {
            self.launcher.execute(run);
            self.hide();
            return;
        }
        self.hide();
        self.launcher.execute(run);
    }

    pub(super) fn accept_typed_text(&self) {
        let session = self.dmenu_session.get();
        if session == 0 {
            return;
        }
        let text = self.entry.text().to_string();
        self.launcher.finish_dmenu(session, Reply::Choice(text));
        self.hide();
    }

    pub(super) fn is_list_wanted(&self) -> bool {
        if self.dmenu_session.get() != 0 {
            return !self.is_dmenu_prompt_only.get() && !self.rows.borrow().is_empty();
        }
        !self.query().is_empty()
    }

    pub(super) fn list_max(&self) -> i32 {
        let lines = i32::try_from(self.dmenu_lines.get()).unwrap_or(i32::MAX);
        let first_row = self.rows.borrow().first().map(|row| row.widget.clone());
        let Some(first_row) = first_row.filter(|_| lines > 0) else {
            return LIST_MAX;
        };
        let row_height = first_row.measure(gtk4::Orientation::Vertical, -1).1 + LIST_SPACING;
        let height = lines
            .saturating_mul(row_height)
            .saturating_add(LIST_MARGIN * 2 - LIST_SPACING);
        height.min(LIST_MAX)
    }
}

impl Drop for Overview {
    fn drop(&mut self) {
        self.launcher
            .finish_dmenu(self.dmenu_session.get(), Reply::Cancellation);
    }
}
