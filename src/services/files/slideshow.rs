use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate::core::config::{self, SlideshowOrder};
use crate::services::files::wallpapers::{Wallpapers, list_pictures};
use crate::services::session::background::BackgroundTasks;
use crate::theming::switchwallqueue;

pub fn start(
    background: &BackgroundTasks,
    wallpapers: &Rc<Wallpapers>,
    is_paused: impl Fn() -> bool + 'static,
) {
    let wallpapers = wallpapers.clone();
    background.add_every_from_now(
        "wallpaper slideshow",
        || {
            let config = config::current();
            let minutes = if config.is_slideshow_enabled {
                config.slideshow_interval_minutes
            } else {
                0
            };
            Duration::from_secs(minutes * 60)
        },
        move || {
            if is_paused() {
                return Ok(());
            }
            advance(&wallpapers);
            Ok(())
        },
    );
}

pub fn advance(wallpapers: &Rc<Wallpapers>) {
    let config = config::current();
    let monitors = if config.is_wallpaper_per_monitor {
        list_monitors()
    } else {
        vec![None]
    };
    let folder = wallpapers.directory();
    glib::spawn_future_local(async move {
        let images = gio::spawn_blocking(move || list_pictures(&folder))
            .await
            .unwrap_or_default();
        let mut taken_images = Vec::new();
        for monitor in monitors {
            let current = config.wallpaper_for(monitor.as_deref().unwrap_or_default());
            let Some(image) = choose_next_image(
                &images,
                Path::new(current),
                &taken_images,
                config.slideshow_order,
                glib::random_int() as usize,
            ) else {
                continue;
            };
            let path = image.to_string_lossy();
            let mut arguments = vec!["--image", &path];
            arguments.extend(
                monitor
                    .iter()
                    .flat_map(|monitor| ["--monitor", monitor.as_str()]),
            );
            switchwallqueue::request(&arguments);
            taken_images.push(image.clone());
        }
    });
}

fn list_monitors() -> Vec<Option<String>> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    display
        .monitors()
        .iter::<gdk::Monitor>()
        .flatten()
        .filter_map(|monitor| monitor.connector())
        .map(|connector| Some(connector.to_string()))
        .collect()
}

fn choose_next_image<'a>(
    images: &'a [PathBuf],
    current: &Path,
    taken_images: &[PathBuf],
    order: SlideshowOrder,
    random_value: usize,
) -> Option<&'a PathBuf> {
    let mut others: Vec<&PathBuf> = images
        .iter()
        .filter(|image| image.as_path() != current)
        .collect();
    if others.is_empty() {
        return None;
    }
    others.sort_by(|left, right| compare_names(left, right));
    let start = match order {
        SlideshowOrder::Name => others
            .iter()
            .filter(|image| compare_names(image, current).is_lt())
            .count(),
        SlideshowOrder::Shuffle => random_value % others.len(),
    };
    let candidates: Vec<&PathBuf> = (0..others.len())
        .map(|offset| others[(start + offset) % others.len()])
        .collect();
    candidates
        .iter()
        .copied()
        .find(|image| !taken_images.contains(image))
        .or_else(|| candidates.first().copied())
}

fn compare_names(left: &Path, right: &Path) -> Ordering {
    let key = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
    };
    key(left).cmp(&key(right)).then_with(|| left.cmp(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_paths(names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|name| Path::new("/w").join(name))
            .collect()
    }

    fn choose(
        images: &[PathBuf],
        current: &str,
        taken_names: &[&str],
        order: SlideshowOrder,
        random_value: usize,
    ) -> Option<String> {
        let taken_images = make_paths(taken_names);
        choose_next_image(
            images,
            Path::new(current),
            &taken_images,
            order,
            random_value,
        )
        .map(|image| image.to_string_lossy().into_owned())
    }

    #[test]
    fn name_order_follows_file_names_ignoring_case_and_wraps_around() {
        let images = make_paths(&["c.png", "A.png", "b.png"]);
        for (current, expected) in [
            ("/w/A.png", "/w/b.png"),
            ("/w/b.png", "/w/c.png"),
            ("/w/c.png", "/w/A.png"),
        ] {
            let chosen = choose(&images, current, &[], SlideshowOrder::Name, 7);
            assert_eq!(chosen.as_deref(), Some(expected), "{current}");
        }
    }

    #[test]
    fn name_order_from_a_wallpaper_outside_the_folder_continues_by_name() {
        let images = make_paths(&["a.png", "c.png", "e.png"]);
        for (current, expected) in [
            ("/elsewhere/d.png", "/w/e.png"),
            ("/elsewhere/z.png", "/w/a.png"),
            ("", "/w/a.png"),
        ] {
            let chosen = choose(&images, current, &[], SlideshowOrder::Name, 0);
            assert_eq!(chosen.as_deref(), Some(expected), "{current}");
        }
    }

    #[test]
    fn shuffle_order_picks_among_the_other_images_by_the_random_value() {
        let images = make_paths(&["a.png", "b.png", "c.png"]);
        for (random_value, expected) in [(0, "/w/a.png"), (1, "/w/c.png"), (2, "/w/a.png")] {
            let chosen = choose(
                &images,
                "/w/b.png",
                &[],
                SlideshowOrder::Shuffle,
                random_value,
            );
            assert_eq!(chosen.as_deref(), Some(expected), "{random_value}");
        }
    }

    #[test]
    fn an_image_another_monitor_just_took_is_skipped_while_another_one_is_free() {
        let images = make_paths(&["a.png", "b.png", "c.png"]);
        for order in [SlideshowOrder::Name, SlideshowOrder::Shuffle] {
            let chosen = choose(&images, "/w/a.png", &["/w/b.png"], order, 0);
            assert_eq!(chosen.as_deref(), Some("/w/c.png"));
        }
        let taken_names = ["/w/b.png", "/w/c.png"];
        let chosen = choose(&images, "/w/a.png", &taken_names, SlideshowOrder::Name, 0);
        assert_eq!(chosen.as_deref(), Some("/w/b.png"));
    }

    #[test]
    fn nothing_changes_without_another_image() {
        for order in [SlideshowOrder::Name, SlideshowOrder::Shuffle] {
            assert_eq!(choose(&[], "/w/a.png", &[], order, 0), None);
            let only = make_paths(&["a.png"]);
            assert_eq!(choose(&only, "/w/a.png", &[], order, 0), None);
        }
    }
}
