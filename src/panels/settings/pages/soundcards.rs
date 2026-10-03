use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::sound::NO_SERVER;
use crate::services::audio::{self, Card};
use crate::ui::widgets::controls::ComboBox;

type Held = Rc<RefCell<Vec<Box<dyn Any>>>>;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    if context.services.audio.is_none() {
        let section = page.section("volume_off", &tr("Sound"));
        page.notice(&section, "info", &tr(NO_SERVER));
        return page;
    }
    let cards = page.section("", "");
    let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    cards.append(&holder);
    load_cards(&page, &holder);
    page
}

fn load_cards(page: &Rc<Page>, holder: &gtk4::Box) {
    let held: Held = Rc::default();
    page.keep(held.clone());
    let (page, holder, held) = (
        Rc::downgrade(page),
        holder.downgrade(),
        Rc::downgrade(&held),
    );
    audio::cards(move |cards: Vec<Card>| {
        if let (Some(page), Some(holder), Some(held)) =
            (page.upgrade(), holder.upgrade(), held.upgrade())
        {
            fill_cards(&page, &holder, &held, cards);
        }
    });
}

fn fill_cards(page: &Rc<Page>, holder: &gtk4::Box, held: &Held, cards: Vec<Card>) {
    while let Some(child) = holder.first_child() {
        holder.remove(&child);
    }
    let mut kept = held.borrow_mut();
    kept.clear();
    for card in cards {
        let (group, tip) = page.unkept_subsection(
            holder,
            &card.description,
            &tr("Which of the card's input and output configurations PipeWire uses"),
        );
        let combo = ComboBox::new(&page.theme);
        combo.set_icon("tune");
        combo.button.set_hexpand(true);
        group.append(&combo.button);
        let labels: Vec<String> = card
            .profiles
            .iter()
            .map(|(label, _)| label.clone())
            .collect();
        let index = card
            .profiles
            .iter()
            .position(|(_, value)| *value == card.active)
            .unwrap_or(0);
        combo.set_items(&labels, index as i32);
        combo.connect_activated({
            let (page, holder) = (Rc::downgrade(page), holder.downgrade());
            let held = Rc::downgrade(held);
            let name = card.name.clone();
            let profiles = card.profiles.clone();
            move |index| {
                let Some((_, profile)) = profiles.get(index) else {
                    return;
                };
                let (page, holder, held) = (page.clone(), holder.clone(), held.clone());
                audio::set_card_profile(&name, profile, move || {
                    audio::cards(move |cards| {
                        if let (Some(page), Some(holder), Some(held)) =
                            (page.upgrade(), holder.upgrade(), held.upgrade())
                        {
                            fill_cards(&page, &holder, &held, cards);
                        }
                    });
                });
            }
        });
        kept.push(Box::new(combo));
        kept.push(Box::new(tip));
    }
}
