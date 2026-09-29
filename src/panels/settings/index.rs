use std::collections::{HashMap, HashSet};
use std::iter::once;

use crate::core::i18n::{self, tr};
use crate::core::shell;
use crate::panels::settings::pages::{self, PAGES, SUBPAGES};

const TRAIL_SEPARATOR: &str = " › ";

pub struct Setting {
    pub page: &'static str,
    pub path: &'static [&'static str],
    pub title: &'static str,
}

const SETTINGS: &[Setting] = include!(concat!(env!("OUT_DIR"), "/settings_index.rs"));

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub page: &'static str,
    pub path: &'static [&'static str],
    pub title: String,
    pub trail: String,
    pub setting: bool,
}

struct Entry {
    hit: Hit,
    titles: Vec<String>,
    rest: String,
}

pub struct Index {
    translations: HashMap<&'static str, Vec<String>>,
}

fn page_name(id: &str) -> &'static str {
    pages::subpage(id)
        .map(|subpage| subpage.title)
        .or_else(|| pages::index_of(id).map(|index| PAGES[index].name))
        .unwrap_or("")
}

fn shown(text: &str) -> String {
    tr(text).replace("%1", shell::name())
}

fn keys() -> HashSet<&'static str> {
    PAGES
        .iter()
        .map(|page| page.name)
        .chain(SUBPAGES.iter().map(|subpage| subpage.title))
        .chain(
            SETTINGS
                .iter()
                .flat_map(|setting| once(setting.title).chain(setting.path.iter().copied())),
        )
        .collect()
}

impl Index {
    pub fn new() -> Self {
        Index {
            translations: i18n::every_translation(&keys()),
        }
    }

    fn variants(&self, text: &'static str) -> impl Iterator<Item = String> + '_ {
        once(text)
            .chain(
                self.translations
                    .get(text)
                    .into_iter()
                    .flatten()
                    .map(String::as_str),
            )
            .map(|variant| variant.replace("%1", shell::name()).to_lowercase())
    }

    fn entry(
        &self,
        hit: Hit,
        title: &'static str,
        trail: &[&'static str],
        keywords: &str,
    ) -> Entry {
        let titles = self
            .variants(title)
            .chain(once(hit.title.to_lowercase()))
            .collect();
        let rest = trail
            .iter()
            .flat_map(|part| self.variants(part))
            .chain(once(keywords.to_lowercase()))
            .collect::<Vec<_>>()
            .join(" ");
        Entry { hit, titles, rest }
    }

    fn entries(&self) -> impl Iterator<Item = Entry> + '_ {
        let rail = PAGES.iter().map(|page| {
            let hit = Hit {
                page: page.id,
                path: &[],
                title: shown(page.name),
                trail: String::new(),
                setting: false,
            };
            self.entry(hit, page.name, &[], &page.keywords.join(" "))
        });
        let subpages = SUBPAGES.iter().map(|subpage| {
            let parent = page_name(subpage.parent);
            let hit = Hit {
                page: subpage.id,
                path: &[],
                title: shown(subpage.title),
                trail: shown(parent),
                setting: false,
            };
            self.entry(hit, subpage.title, &[parent], "")
        });
        let settings = SETTINGS
            .iter()
            .filter(|setting| !page_name(setting.page).is_empty())
            .map(|setting| {
                let trail: Vec<&'static str> = once(page_name(setting.page))
                    .chain(setting.path.iter().copied())
                    .collect();
                let hit = Hit {
                    page: setting.page,
                    path: setting.path,
                    title: shown(setting.title),
                    trail: trail
                        .iter()
                        .map(|part| shown(part))
                        .collect::<Vec<_>>()
                        .join(TRAIL_SEPARATOR),
                    setting: true,
                };
                self.entry(hit, setting.title, &trail, "")
            });
        rail.chain(subpages).chain(settings)
    }

    pub fn search(&self, query: &str) -> Vec<Hit> {
        let query = query.trim().to_lowercase();
        let terms: Vec<&str> = query.split_whitespace().collect();
        if terms.is_empty() {
            return Vec::new();
        }
        let holds_every_term = |text: &str| terms.iter().all(|term| text.contains(term));
        let mut found: Vec<(u8, Hit)> = self
            .entries()
            .filter_map(|entry| {
                let rank = if entry.titles.iter().any(|title| title.starts_with(&query)) {
                    0
                } else if entry.titles.iter().any(|title| holds_every_term(title)) {
                    1
                } else if holds_every_term(&format!("{} {}", entry.titles.join(" "), entry.rest)) {
                    2
                } else {
                    return None;
                };
                Some((rank, entry.hit))
            })
            .collect();
        found.sort_by_key(|(rank, _)| *rank);
        found.into_iter().map(|(_, hit)| hit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(language: &str, query: &str) -> Vec<Hit> {
        i18n::use_language(language);
        Index::new().search(query)
    }

    fn titles(query: &str) -> Vec<String> {
        search("en_US", query)
            .into_iter()
            .map(|hit| hit.title)
            .collect()
    }

    #[test]
    fn the_lock_screen_blur_switch_is_found_under_its_subpage() {
        let hit = search("en_US", "enable blur")
            .into_iter()
            .find(|hit| hit.page == "lock")
            .expect("the lock blur switch");
        assert_eq!(
            hit,
            Hit {
                page: "lock",
                path: &["Style: Blurred"],
                title: "Enable blur".to_owned(),
                trail: "Screen Lock › Style: Blurred".to_owned(),
                setting: true,
            }
        );
    }

    #[test]
    fn titles_that_start_with_the_query_come_first_then_titles_holding_it_then_trails() {
        let found = search("en_US", "blur");
        let ranks: Vec<u8> = found
            .iter()
            .map(|hit| {
                let title = hit.title.to_lowercase();
                if title.starts_with("blur") {
                    0
                } else if title.contains("blur") {
                    1
                } else {
                    2
                }
            })
            .collect();
        assert!(ranks.windows(2).all(|pair| pair[0] <= pair[1]), "{ranks:?}");
        assert!(ranks.contains(&2));
        assert_eq!(found[0].title, "Blur");
    }

    #[test]
    fn pages_are_found_by_name_and_keyword() {
        let net = titles("net");
        assert_eq!(net[0], "Network");
        assert!(net.contains(&"Wi-Fi".to_owned()));
        assert!(titles("volume").contains(&"Sound".to_owned()));
        assert!(titles("  ").is_empty());
        assert!(titles("nothing like this").is_empty());
    }

    #[test]
    fn a_shell_name_placeholder_reads_as_the_shell() {
        assert!(
            titles("hyprlock").contains(&format!("Use Hyprlock (instead of {})", shell::name()))
        );
    }

    #[test]
    fn a_query_in_another_language_finds_the_setting_shown_in_the_interface_language() {
        assert_eq!(titles("мышь")[0], "Mouse & Touchpad");
        let hit = search("ru_RU", "enable blur")
            .into_iter()
            .find(|hit| hit.page == "lock")
            .expect("the lock blur switch");
        assert_eq!(hit.title, "Включить размытие");
        assert_eq!(hit.trail, "Блокировка экрана › Размытие");
        assert_eq!(search("ru_RU", "mouse")[0].title, "Мышь и тачпад");
    }
}
