use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::{tr, trn};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::panels::settings::Settings;
use crate::panels::sidebar::quicktoggle::{Glyph, Look, QuickToggle, Start};
use crate::services::updates::Updates;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::layout::group::ButtonGroup;

const SPACING: f64 = 6.0;
const TILE_SPAN: i32 = 2;
const UPDATES_PAGE: &str = "updates";

pub fn build(
    theme: &SharedTheme,
    updates: &Updates,
    settings: &Rc<Settings>,
    width: f64,
    close: Rc<dyn Fn()>,
    scope: &Scope,
) -> gtk4::Widget {
    let group = ButtonGroup::new(theme);
    group.set_spacing(SPACING);
    group.set_halign(gtk4::Align::Start);

    let tile_width = (width - SPACING) / 2.0;
    let add_tile = || {
        let toggle = QuickToggle::new(
            theme,
            tile_width,
            TILE_SPAN,
            false,
            Glyph::Symbol,
            Start {
                slide_from: None,
                should_fade_in: false,
            },
        );
        let open_page = {
            let close = close.clone();
            let settings = settings.clone();
            Rc::new(move || {
                close();
                settings.open(Some(UPDATES_PAGE));
            })
        };
        toggle.connect_actions(open_page, None);
        group.append(&toggle.button);
        toggle
    };
    let shell = add_tile();
    let system = add_tile();

    let show = {
        let group = group.clone();
        let updates = updates.clone();
        move || {
            let is_busy = updates.job.get().is_some();
            group.set_visible(updates.is_shell_behind() || updates.is_upgrade_advised());
            shell.show(&shell_look(updates.commits_behind.get(), is_busy));
            system.show(&system_look(
                updates.count(),
                updates.has_package_manager.get(),
                updates.has_package_check_failed.get(),
                updates.is_upgrade_strongly_advised(),
                is_busy,
            ));
        }
    };
    show();
    scope.keep(updates.subscribe(show.clone()));
    scope.hold(watch::config("/updates", show));

    group.upcast()
}

fn shell_look(commits_behind: i32, is_busy: bool) -> Look {
    let status = if is_busy {
        tr("Updating…")
    } else if commits_behind > 0 {
        trn(
            "%1 commit behind",
            "%1 commits behind",
            commits_behind as u64,
        )
    } else {
        tr("Up to date")
    };
    Look {
        name: "Shell update",
        tooltip: String::new(),
        status,
        has_status: true,
        icon: "upgrade".to_owned(),
        is_toggled: false,
        is_available: !is_busy && commits_behind > 0,
    }
}

fn system_look(
    count: i32,
    is_available: bool,
    has_check_failed: bool,
    is_advised: bool,
    is_busy: bool,
) -> Look {
    let status = if is_busy {
        tr("Updating…")
    } else if count > 0 {
        trn("%1 package", "%1 packages", count as u64)
    } else if has_check_failed {
        tr("Could not check for updates")
    } else {
        tr("Up to date")
    };
    Look {
        name: "System update",
        tooltip: String::new(),
        status,
        has_status: true,
        icon: "deployed_code_update".to_owned(),
        is_toggled: is_advised,
        is_available: !is_busy && is_available && count > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::settings::pages;

    #[test]
    fn a_tile_opens_a_settings_page_that_exists() {
        assert!(pages::subpage(UPDATES_PAGE).is_some());
    }

    #[test]
    fn a_tile_has_no_tooltip() {
        for look in [
            shell_look(3, false),
            system_look(157, true, false, true, false),
        ] {
            assert!(look.tooltip.is_empty());
        }
    }

    #[test]
    fn a_running_update_disables_both_tiles() {
        let mut shell = shell_look(3, false);
        let mut system = system_look(157, true, false, true, false);
        assert!(shell.is_available && system.is_available);
        shell.status = tr("Updating…");
        shell.is_available = false;
        system.status = tr("Updating…");
        system.is_available = false;
        assert_eq!(shell_look(3, true), shell);
        assert_eq!(system_look(157, true, false, true, true), system);
    }

    #[test]
    fn a_failed_package_check_is_not_shown_as_up_to_date() {
        assert_eq!(
            system_look(0, true, true, false, false).status,
            tr("Could not check for updates")
        );
        assert_eq!(
            system_look(0, true, false, false, false).status,
            tr("Up to date")
        );
    }
}
