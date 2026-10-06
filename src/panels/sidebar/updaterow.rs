use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::panels::settings::Settings;
use crate::panels::sidebar::quicktoggle::{Glyph, Look, QuickToggle, Start};
use crate::services::updates::{Job, Updates};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::group::ButtonGroup;

const SPACING: f64 = 6.0;
const WIDE: i32 = 2;

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
    let tile = |job: Job| {
        let toggle = QuickToggle::new(
            theme,
            tile_width,
            WIDE,
            false,
            Glyph::Symbol,
            Start {
                slide_from: None,
                fade_in: false,
            },
        );
        let close = close.clone();
        let updates = updates.clone();
        let settings = settings.clone();
        toggle.connect_actions(
            Rc::new(move || {
                close();
                updates.start(job);
                settings.open(Some("updates"));
            }),
            None,
        );
        group.append(&toggle.button);
        toggle
    };
    let shell = tile(Job::Shell);
    let system = tile(Job::System);

    let show = {
        let group = group.clone();
        let updates = updates.clone();
        move || {
            let behind = updates.behind.get();
            let count = updates.count();
            group.set_visible(updates.shell_behind() || updates.advised());
            shell.show(&Look {
                name: "Shell update",
                status: if behind > 0 {
                    trf("%1 commits behind", &[&behind.to_string()])
                } else {
                    tr("Up to date")
                },
                has_status: true,
                icon: "upgrade".to_owned(),
                tooltip: String::new(),
                toggled: false,
                available: behind > 0,
            });
            system.show(&Look {
                name: "System update",
                status: if count > 0 {
                    trf("%1 packages", &[&count.to_string()])
                } else {
                    tr("Up to date")
                },
                has_status: true,
                icon: "deployed_code_update".to_owned(),
                tooltip: String::new(),
                toggled: updates.strongly_advised(),
                available: updates.available.get() && count > 0,
            });
        }
    };
    show();
    scope.keep(updates.subscribe(show.clone()));
    scope.hold(watch::config("/updates", show));

    group.upcast()
}
