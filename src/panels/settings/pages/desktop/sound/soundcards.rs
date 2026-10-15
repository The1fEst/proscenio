use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::desktop::sound::NO_SERVER;
use crate::services::media::audio::{self, Card};
use crate::ui::widgets::input::controls::ComboBox;

type Held = Rc<RefCell<Vec<Box<dyn Any>>>>;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    if context.services.audio.is_none() {
        let section = page.section("volume_off", &tr("Sound"));
        page.notice(&section, "info", &tr(NO_SERVER));
        return page;
    }
    let cards_section = page.section("", "");
    let card_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    cards_section.append(&card_box);
    load_cards(&page, &card_box);
    page
}

fn load_cards(page: &Rc<Page>, card_box: &gtk4::Box) {
    let owned_items: Held = Rc::default();
    page.keep(owned_items.clone());
    let (page, card_box, owned_items) = (
        Rc::downgrade(page),
        card_box.downgrade(),
        Rc::downgrade(&owned_items),
    );
    audio::read_cards(move |cards: Vec<Card>| {
        if let (Some(page), Some(card_box), Some(owned_items)) =
            (page.upgrade(), card_box.upgrade(), owned_items.upgrade())
        {
            fill_cards(&page, &card_box, &owned_items, cards);
        }
    });
}

fn fill_cards(page: &Rc<Page>, card_box: &gtk4::Box, owned_items: &Held, cards: Vec<Card>) {
    while let Some(child) = card_box.first_child() {
        card_box.remove(&child);
    }
    let mut items = owned_items.borrow_mut();
    items.clear();
    for card in cards {
        let (group, tip) = page.unkept_subsection(
            card_box,
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
            .position(|(_, value)| *value == card.active_profile)
            .unwrap_or(0);
        combo.set_items(&labels, index as i32);
        combo.connect_activated({
            let (page, card_box) = (Rc::downgrade(page), card_box.downgrade());
            let owned_items = Rc::downgrade(owned_items);
            let name = card.name.clone();
            let profiles = card.profiles.clone();
            move |index| {
                let Some((_, profile)) = profiles.get(index) else {
                    return;
                };
                let (page, card_box, owned_items) =
                    (page.clone(), card_box.clone(), owned_items.clone());
                audio::set_card_profile(&name, profile, move || {
                    audio::read_cards(move |cards| {
                        if let (Some(page), Some(card_box), Some(owned_items)) =
                            (page.upgrade(), card_box.upgrade(), owned_items.upgrade())
                        {
                            fill_cards(&page, &card_box, &owned_items, cards);
                        }
                    });
                });
            }
        });
        items.push(Box::new(combo));
        items.push(Box::new(tip));
    }
}
