use gtk4::gdk::{self, RGBA};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::card::Card;
use super::{ART_SIDE, ELEVATION, HEIGHT, WIDTH};
use crate::core::process;
use crate::ui::theme::{self, SharedTheme, mix};

const DOMINANT_PART: f32 = 0.8;
const ART_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(60);

type ArtWaiter = Box<dyn FnOnce(Option<PathBuf>)>;

thread_local! {
    static FETCH_WAITERS: RefCell<HashMap<PathBuf, Vec<ArtWaiter>>> = RefCell::default();
}

impl Card {
    pub(super) fn follow_art(self: &Rc<Self>, theme: &SharedTheme, url: &str) {
        let is_known_url = self.art_url.borrow().as_deref() == Some(url);
        if !is_known_url {
            self.art_url.replace(Some(url.to_owned()));
            self.art_retry_time_micros.set(None);
            self.dominant_color.set(None);
            self.look.texture.replace(None);
            self.look.thumbnail.replace(None);
            self.look.blur_cache.replace(None);
            self.fade_art(false);
            if !url.is_empty() {
                self.download(theme, url);
            }
        } else if is_retry_due(self.art_retry_time_micros.get(), glib::monotonic_time()) {
            self.art_retry_time_micros.set(None);
            self.download(theme, url);
        }
        let theme_ref = theme.borrow();
        let color = match (url.is_empty(), self.dominant_color.get()) {
            (true, _) => theme_ref.m3.secondary_container,
            (false, Some(average)) => mix(
                average,
                theme_ref.colors.col_primary_container,
                DOMINANT_PART,
            ),
            (false, None) => mix(
                theme_ref.colors.col_primary,
                theme_ref.colors.col_primary_container,
                DOMINANT_PART,
            ),
        };
        self.look
            .adapted_colors
            .set(theme_ref.adapt_colors_to(color));
    }

    fn download(self: &Rc<Self>, theme: &SharedTheme, url: &str) {
        let cache = crate::core::paths::cache().join("coverart");
        let _ = std::fs::create_dir_all(&cache);
        let file = cache.join(
            glib::compute_checksum_for_string(glib::ChecksumType::Md5, url)
                .map(|sum| sum.to_string())
                .unwrap_or_default(),
        );
        let card = Rc::downgrade(self);
        let theme = theme.clone();
        let requested_url = url.to_owned();
        let finish = move |file: Option<PathBuf>| {
            let card = card.clone();
            let theme = theme.clone();
            let requested_url = requested_url.clone();
            let scale = card.upgrade().map_or(1, |card| card.art.scale_factor());
            glib::spawn_future_local(async move {
                let sizes = vec![
                    (
                        (WIDTH - 2 * ELEVATION) * scale,
                        (HEIGHT - 2 * ELEVATION) * scale,
                    ),
                    (ART_SIDE * scale, ART_SIDE * scale),
                ];
                let cover = match file {
                    Some(file) => decode(file, sizes).await,
                    None => None,
                };
                let Some(card) = card.upgrade() else {
                    return;
                };
                if card.art_url.borrow().as_deref() != Some(requested_url.as_str()) {
                    return;
                }
                let Some((textures, dominant_color)) = cover else {
                    card.art_retry_time_micros.set(Some(
                        glib::monotonic_time() + ART_RETRY_DELAY.as_micros() as i64,
                    ));
                    return;
                };
                let mut textures = textures.into_iter();
                card.dominant_color.set(dominant_color);
                card.look.texture.replace(textures.next());
                card.look.thumbnail.replace(textures.next());
                card.follow_art(&theme, &requested_url);
                card.recolor(card.root.is_mapped());
                card.fade_art(true);
            });
        };
        if file.exists() {
            finish(Some(file));
            return;
        }
        let is_first = FETCH_WAITERS.with_borrow_mut(|fetch_waiters| {
            let waiters = fetch_waiters.entry(file.clone()).or_default();
            waiters.push(Box::new(finish));
            waiters.len() == 1
        });
        if !is_first {
            return;
        }
        let url = url.to_owned();
        glib::spawn_future_local(async move {
            let is_fetched = fetch(&url, &file).await;
            let waiters = FETCH_WAITERS
                .with_borrow_mut(|fetch_waiters| fetch_waiters.remove(&file))
                .unwrap_or_default();
            for finish in waiters {
                finish(is_fetched.then(|| file.clone()));
            }
        });
    }

    fn fade_art(self: &Rc<Self>, should_show: bool) {
        let target = if should_show { 1.0 } else { 0.0 };
        let mut opacity = self.look.opacity.get();
        if self.root.is_mapped() {
            opacity.retarget(target, glib::monotonic_time());
        } else {
            opacity.jump(target);
        }
        self.look.opacity.set(opacity);
        self.backdrop.queue_draw();
        self.art.queue_draw();
        if self.is_ticking.replace(true) {
            return;
        }
        let card = Rc::downgrade(self);
        self.root.add_tick_callback(move |_, _| {
            let Some(card) = card.upgrade() else {
                return glib::ControlFlow::Break;
            };
            card.backdrop.queue_draw();
            card.art.queue_draw();
            if card.look.opacity.get().is_running(glib::monotonic_time()) {
                return glib::ControlFlow::Continue;
            }
            card.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }
}

fn is_retry_due(retry_time_micros: Option<i64>, now_micros: i64) -> bool {
    retry_time_micros.is_some_and(|time_micros| now_micros >= time_micros)
}

async fn decode(
    file: PathBuf,
    sizes: Vec<(i32, i32)>,
) -> Option<(Vec<gdk::Texture>, Option<RGBA>)> {
    let cover = crate::ui::image::cover_textures(file.clone(), sizes, |pixbuf| {
        theme::average(pixbuf.clone())
    })
    .await;
    if cover.is_none() {
        let _ = std::fs::remove_file(&file);
    }
    cover
}

async fn fetch(url: &str, file: &Path) -> bool {
    let part_file = file.with_extension(format!("{}.part", std::process::id()));
    let download = process::quiet(&[
        OsStr::new("curl"),
        OsStr::new("-4"),
        OsStr::new("-fsSL"),
        OsStr::new("--max-time"),
        OsStr::new("30"),
        OsStr::new(url),
        OsStr::new("-o"),
        part_file.as_os_str(),
    ]);
    let is_fetched =
        process::finish(download).await == Some(true) && std::fs::rename(&part_file, file).is_ok();
    if !is_fetched {
        let _ = std::fs::remove_file(&part_file);
    }
    is_fetched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_art_is_cached_only_when_the_download_succeeds() {
        let folder = std::env::temp_dir().join(format!("proscenio-cover-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("source.png");
        std::fs::write(&source, b"cover").unwrap();
        let context = glib::MainContext::new();

        let missing_file = folder.join("missing");
        let url = format!("file://{}", folder.join("absent.png").display());
        assert!(!context.block_on(fetch(&url, &missing_file)));
        assert!(!missing_file.exists());

        let cache_file = folder.join("cached");
        let url = format!("file://{}", source.display());
        assert!(context.block_on(fetch(&url, &cache_file)));
        assert_eq!(std::fs::read(&cache_file).unwrap(), b"cover");

        let mut file_names: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        file_names.sort();
        assert_eq!(file_names, ["cached", "source.png"]);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn missing_art_is_fetched_again_only_once_its_wait_is_over() {
        assert!(!is_retry_due(None, 100));
        assert!(!is_retry_due(Some(200), 100));
        assert!(is_retry_due(Some(200), 200));
        assert!(is_retry_due(Some(200), 300));
    }

    #[test]
    fn cached_art_that_does_not_decode_is_removed() {
        let file =
            std::env::temp_dir().join(format!("proscenio-cover-broken-{}", std::process::id()));
        std::fs::write(&file, b"<html>404</html>").unwrap();
        let cover = glib::MainContext::new().block_on(decode(file.clone(), vec![(10, 10)]));
        let file_exists = file.exists();
        let _ = std::fs::remove_file(&file);
        assert!(cover.is_none());
        assert!(!file_exists);
    }
}
