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

fn page_name(id: &str) -> &'static str {
    pages::subpage(id)
        .map(|subpage| subpage.title)
        .or_else(|| pages::index_of(id).map(|index| PAGES[index].name))
        .unwrap_or("")
}

fn hits() -> impl Iterator<Item = (Hit, String)> {
    let rail = PAGES.iter().map(|page| {
        let hit = Hit {
            page: page.id,
            path: &[],
            title: page.name.to_owned(),
            trail: String::new(),
            setting: false,
        };
        (hit, page.keywords.join(" "))
    });
    let subpages = SUBPAGES.iter().map(|subpage| {
        let hit = Hit {
            page: subpage.id,
            path: &[],
            title: subpage.title.to_owned(),
            trail: page_name(subpage.parent).to_owned(),
            setting: false,
        };
        (hit, String::new())
    });
    let settings = SETTINGS
        .iter()
        .filter(|setting| !page_name(setting.page).is_empty())
        .map(|setting| {
            let trail = std::iter::once(page_name(setting.page))
                .chain(setting.path.iter().copied())
                .collect::<Vec<_>>()
                .join(TRAIL_SEPARATOR);
            let hit = Hit {
                page: setting.page,
                path: setting.path,
                title: setting.title.replace("%1", shell::name()),
                trail,
                setting: true,
            };
            (hit, String::new())
        });
    rail.chain(subpages).chain(settings)
}

pub fn search(query: &str) -> Vec<Hit> {
    let query = query.trim().to_lowercase();
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(u8, Hit)> = hits()
        .filter_map(|(hit, keywords)| {
            let title = hit.title.to_lowercase();
            let everything = format!("{title} {} {keywords}", hit.trail.to_lowercase());
            if !terms.iter().all(|term| everything.contains(term)) {
                return None;
            }
            let rank = if title.starts_with(&query) {
                0
            } else if terms.iter().all(|term| title.contains(term)) {
                1
            } else {
                2
            };
            Some((rank, hit))
        })
        .collect();
    found.sort_by_key(|(rank, _)| *rank);
    found.into_iter().map(|(_, hit)| hit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(query: &str) -> Vec<String> {
        search(query).into_iter().map(|hit| hit.title).collect()
    }

    #[test]
    fn the_lock_screen_blur_switch_is_found_under_its_subpage() {
        let hit = search("enable blur")
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
        let found = search("blur");
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
}
