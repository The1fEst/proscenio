use gtk4::prelude::*;
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::desktop::sound::NO_SERVER;
use crate::panels::sound::streams::StreamList;
use crate::ui::widgets::coalesce;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let Some(audio) = context.services.audio.clone() else {
        let section = page.section("volume_off", &tr("Sound"));
        page.notice(&section, "info", &tr(NO_SERVER));
        return page;
    };
    let mpris = context.services.mpris.clone();

    let lists = [true, false].map(|is_sink| {
        let (icon, title) = if is_sink {
            ("volume_up", "Playback")
        } else {
            ("mic", "Recording")
        };
        let list = StreamList::new(&context.theme, &audio, &mpris, is_sink);
        page.section(icon, &tr(title)).append(&list.root);
        list
    });
    let refresh = {
        let lists = lists.each_ref().map(Rc::downgrade);
        move || {
            for list in lists.iter().filter_map(Weak::upgrade) {
                list.refresh();
            }
        }
    };
    refresh();
    let schedule_refresh = coalesce(Rc::new(refresh));
    page.keep(audio.watch({
        let schedule_refresh = schedule_refresh.clone();
        move || schedule_refresh()
    }));
    page.keep(mpris.subscribe(move || schedule_refresh()));
    page.keep(lists);
    page
}
