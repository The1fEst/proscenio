use gtk4::gdk_pixbuf::{Colorspace, Pixbuf};
use gtk4::gio;
use gtk4::glib::{self, Variant};
use std::path::{Path, PathBuf};

use super::store::write_store;
use super::{Notifications, hint, hint_string};
use crate::platform::system::desktop;

const PIXEL_HINTS: [&str; 2] = ["image-data", "image_data"];
const PATH_HINTS: [&str; 2] = ["image-path", "image_path"];
const ICON_PIXEL_HINT: &str = "icon_data";

impl Notifications {
    pub(super) fn take_image_request(&self) -> u64 {
        let request = self.next_image_request.get();
        self.next_image_request.set(request + 1);
        request
    }

    pub(super) fn apply_image(
        &self,
        id: u32,
        request: u64,
        image: String,
    ) -> (bool, Option<String>) {
        let is_newer = self
            .applied_image_requests
            .borrow()
            .get(&id)
            .is_none_or(|applied_request| request > *applied_request);
        if !is_newer {
            return (false, Some(image));
        }
        let mut list = self.list.borrow_mut();
        let Some(notification) = list.iter_mut().find(|notification| notification.id == id) else {
            return (false, Some(image));
        };
        self.applied_image_requests.borrow_mut().insert(id, request);
        if notification.image == image {
            return (false, None);
        }
        (
            true,
            Some(std::mem::replace(&mut notification.image, image)),
        )
    }

    pub(super) fn request_image(&self, id: u32, hints: Variant, app_icon: String) {
        let request = self.take_image_request();
        let has_image_hint = PIXEL_HINTS
            .iter()
            .chain(app_icon.is_empty().then_some(&ICON_PIXEL_HINT))
            .any(|key| hint(&hints, key).is_some())
            || PATH_HINTS.iter().any(|key| {
                hint_string(&hints, key).is_some_and(|location| !location.trim().is_empty())
            });
        if !has_image_hint {
            self.finish_image_request(id, request, String::new());
            return;
        }
        let png_path = image_dir().join(format!("{id}-{request}.png"));
        let notifications = self.clone();
        glib::spawn_future_local(async move {
            let Ok(image) =
                gio::spawn_blocking(move || resolve_image(&png_path, &hints, &app_icon)).await
            else {
                return;
            };
            notifications.finish_image_request(id, request, image);
        });
    }

    fn finish_image_request(&self, id: u32, request: u64, image: String) {
        let (is_applied, unused_image) = self.apply_image(id, request, image);
        if is_applied {
            write_store(&self.list.borrow());
            self.announce();
        }
        if let Some(unused_image) = unused_image {
            remove_cached_image(id, &unused_image);
        }
    }
}

pub(super) fn resolve_file_location(location: String) -> String {
    if is_file_uri(&location) || location.starts_with('/') {
        resolve_image_location(&location).unwrap_or_default()
    } else {
        location
    }
}

pub(super) fn resolve_app_icon(icon: String, hints: &Variant, app_name: &str) -> String {
    let icon = resolve_file_location(icon);
    if !icon.is_empty() {
        return icon;
    }
    hint_string(hints, "desktop-entry")
        .and_then(|id| desktop::find(&id))
        .map(|entry| entry.icon())
        .unwrap_or_else(|| named_icon(app_name))
}

pub(super) fn named_icon(app_name: &str) -> String {
    if app_name.is_empty() {
        return String::new();
    }
    desktop::find(app_name)
        .or_else(|| {
            desktop::all()
                .into_iter()
                .find(|entry| entry.name().eq_ignore_ascii_case(app_name))
        })
        .map(|entry| entry.icon())
        .unwrap_or_default()
}

fn resolve_image(png_path: &Path, hints: &Variant, app_icon: &str) -> String {
    let save_first_pixels = |keys: &[&str]| {
        keys.iter()
            .filter_map(|key| hint(hints, key))
            .find_map(|pixels| save_pixels(png_path, &pixels))
    };
    save_first_pixels(&PIXEL_HINTS)
        .or_else(|| {
            PATH_HINTS
                .into_iter()
                .filter_map(|key| hint_string(hints, key))
                .find_map(|location| resolve_image_location(&location))
        })
        .or_else(|| {
            if app_icon.is_empty() {
                save_first_pixels(&[ICON_PIXEL_HINT])
            } else {
                None
            }
        })
        .unwrap_or_default()
}

pub fn is_file_uri(location: &str) -> bool {
    location
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
}

pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let (path, host) = glib::filename_from_uri(uri).ok()?;
    host.is_none_or(|host| {
        host.eq_ignore_ascii_case("localhost") || host.eq_ignore_ascii_case(&glib::host_name())
    })
    .then_some(path)
}

pub(super) fn resolve_image_location(location: &str) -> Option<String> {
    if location.is_empty() {
        return None;
    }
    let path = if is_file_uri(location) {
        file_uri_to_path(location)?
    } else if location.starts_with('/') {
        PathBuf::from(location)
    } else if location.contains(['/', ':'])
        || location.starts_with('~')
        || location.contains(char::is_whitespace)
    {
        return None;
    } else {
        return Some(location.to_owned());
    };
    if !path.is_file() {
        return None;
    }
    path.into_os_string().into_string().ok()
}

fn save_pixels(path: &Path, pixels: &Variant) -> Option<String> {
    let (width, height, rowstride, has_alpha, bits, channels, data) =
        pixels.get::<(i32, i32, i32, bool, i32, i32, Vec<u8>)>()?;
    if bits != 8 || width < 1 || height < 1 || channels != if has_alpha { 4 } else { 3 } {
        return None;
    }
    let row = width.checked_mul(channels)?;
    let size = rowstride.checked_mul(height - 1)?.checked_add(row)?;
    if rowstride < row || data.len() < size as usize {
        return None;
    }
    let pixbuf = Pixbuf::from_bytes(
        &glib::Bytes::from_owned(data),
        Colorspace::Rgb,
        has_alpha,
        bits,
        width,
        height,
        rowstride,
    );
    std::fs::create_dir_all(path.parent()?).ok()?;
    let partial_path = path.with_extension("png.part");
    if pixbuf.savev(&partial_path, "png", &[]).is_err()
        || std::fs::rename(&partial_path, path).is_err()
    {
        let _ = std::fs::remove_file(&partial_path);
        return None;
    }
    Some(path.to_string_lossy().into_owned())
}

fn image_dir() -> PathBuf {
    crate::core::paths::cache().join("notifications")
}

pub(super) fn remove_discarded_images(id: u32, image: &str) {
    remove_cached_image(id, image);
    let _ = std::fs::remove_file(image_dir().join(format!("{id}.png")));
}

fn remove_cached_image(id: u32, image: &str) {
    if is_own_image(&image_dir(), id, image) {
        let _ = std::fs::remove_file(image);
    }
}

fn is_own_image(dir: &Path, id: u32, image: &str) -> bool {
    let path = Path::new(image);
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(request) = file_name
        .strip_suffix(".png")
        .and_then(|stem| stem.strip_prefix(&id.to_string()))
    else {
        return false;
    };
    let is_request_number = request.strip_prefix('-').is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    });
    path.parent() == Some(dir) && (request.is_empty() || is_request_number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::*;
    use std::collections::HashMap;

    #[test]
    fn image_path_hints_resolve_to_an_icon_name_or_an_existing_file() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-image-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let spaced_file = folder.join("a b.png");
        let cover = folder.join("cover.png");
        std::fs::write(&spaced_file, b"png").unwrap();
        std::fs::write(&cover, b"png").unwrap();
        let to_uri = |path: &Path| gio::File::for_path(path).uri().to_string();
        let to_text = |path: &Path| path.to_string_lossy().into_owned();
        let cases = [
            ("image-path", to_uri(&spaced_file), to_text(&spaced_file)),
            ("image_path", to_uri(&cover), to_text(&cover)),
            (
                "image-path",
                to_uri(&cover).replacen("file", "FILE", 1),
                to_text(&cover),
            ),
            ("image-path", to_text(&cover), to_text(&cover)),
            (
                "image-path",
                "audio-x-generic".to_owned(),
                "audio-x-generic".to_owned(),
            ),
            (
                "image-path",
                to_text(&folder.join("gone.png")),
                String::new(),
            ),
            ("image-path", to_text(&folder), String::new()),
            ("image-path", "covers/cover.png".to_owned(), String::new()),
            ("image-path", "~/cover.png".to_owned(), String::new()),
            ("image-path", " ".to_owned(), String::new()),
            ("image-path", " audio-x-generic\n".to_owned(), String::new()),
            ("image-path", "mailto:x".to_owned(), String::new()),
            (
                "image-path",
                "file://x.invalid/srv/cover.png".to_owned(),
                String::new(),
            ),
            (
                "image-path",
                format!("file://LocalHost{}", to_text(&cover)),
                to_text(&cover),
            ),
        ];
        let png_path = Path::new("/nonexistent/proscenio/0.png");
        for (key, given, expected) in cases {
            let hints = HashMap::from([(key.to_owned(), given.to_variant())]).to_variant();
            assert_eq!(resolve_image(png_path, &hints, ""), expected, "{given}");
        }
        let empty = HashMap::<String, Variant>::new().to_variant();
        assert_eq!(resolve_image(png_path, &empty, ""), "");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn an_unusable_image_hint_falls_through_to_the_next_one() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-fallthrough-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let png_path = folder.join("0.png");
        let cover = folder.join("cover.png");
        std::fs::write(&cover, b"png").unwrap();
        let cover_text = cover.to_string_lossy().into_owned();
        let build_pixels = |channels: i32| (1, 1, 4, true, 8, channels, vec![0u8; 4]).to_variant();
        let build_hints = |pairs: &[(&str, Variant)]| {
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect::<HashMap<String, Variant>>()
                .to_variant()
        };
        let png_text = png_path.to_string_lossy().into_owned();
        let cases = [
            (
                build_hints(&[
                    ("image-data", build_pixels(3)),
                    ("image_data", build_pixels(4)),
                ]),
                "",
                png_text.clone(),
            ),
            (
                build_hints(&[
                    ("image-data", build_pixels(3)),
                    ("image-path", cover_text.to_variant()),
                    ("icon_data", build_pixels(4)),
                ]),
                "",
                cover_text.clone(),
            ),
            (
                build_hints(&[
                    (
                        "image-path",
                        folder.join("gone.png").to_string_lossy().to_variant(),
                    ),
                    ("image_path", cover_text.to_variant()),
                ]),
                "",
                cover_text.clone(),
            ),
            (
                build_hints(&[("icon_data", build_pixels(4))]),
                "",
                png_text.clone(),
            ),
            (
                build_hints(&[("icon_data", build_pixels(4))]),
                "firefox",
                String::new(),
            ),
        ];
        for (hints, app_icon, expected) in cases {
            assert_eq!(
                resolve_image(&png_path, &hints, app_icon),
                expected,
                "{hints} {app_icon}"
            );
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_file_app_icon_becomes_an_existing_local_path() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-app-icon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let icon_file = folder.join("a b.png");
        std::fs::write(&icon_file, b"png").unwrap();
        let icon_text = icon_file.to_string_lossy().into_owned();
        let uri_path = gio::File::for_path(&icon_file)
            .uri()
            .trim_start_matches("file://")
            .to_owned();
        let hints = HashMap::<String, Variant>::new().to_variant();
        let cases = [
            (format!("file://{uri_path}"), icon_text.clone()),
            (
                format!("file://{}{uri_path}", glib::host_name()),
                icon_text.clone(),
            ),
            ("file://x.invalid/srv/icon.png".to_owned(), String::new()),
            (
                folder.join("gone.png").to_string_lossy().into_owned(),
                String::new(),
            ),
            ("firefox".to_owned(), "firefox".to_owned()),
        ];
        for (icon, expected) in cases {
            assert_eq!(
                resolve_app_icon(icon.clone(), &hints, ""),
                expected,
                "{icon}"
            );
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn only_a_notifications_own_cache_path_counts_as_its_image() {
        let dir = Path::new("/cache/notifications");
        let cases = [
            ("/cache/notifications/3-17.png", true),
            ("/cache/notifications/3.png", true),
            ("/cache/notifications/4-17.png", false),
            ("/cache/notifications/34-17.png", false),
            ("/cache/notifications/3-foo.png", false),
            ("/cache/notifications/3-.png", false),
            ("/cache/notifications/3-17.png.part", false),
            ("/elsewhere/3-17.png", false),
            ("", false),
        ];
        for (image, is_own) in cases {
            assert_eq!(is_own_image(dir, 3, image), is_own, "{image}");
        }
    }

    #[test]
    fn only_well_formed_image_data_is_saved() {
        let folder = std::env::temp_dir().join(format!("proscenio-pixels-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let cases = [
            (1, 1, 4, true, 8, 4, true),
            (1, 1, 4, false, 8, 3, true),
            (1, 0, 4, true, 8, 4, false),
            (1, -1, 4, true, 8, 4, false),
            (1, i32::MIN, 4, true, 8, 4, false),
            (0, 1, 4, true, 8, 4, false),
            (i32::MIN, 1, 4, true, 8, 4, false),
            (1, 1, 4, true, 16, 4, false),
            (1, 1, 0, true, 8, 4, false),
            (1, 1, 4, true, 8, 3, false),
            (1, 1, 4, false, 8, 4, false),
        ];
        for (index, (width, height, rowstride, has_alpha, bits, channels, is_saved)) in
            cases.into_iter().enumerate()
        {
            let path = folder.join(format!("{index}.png"));
            let pixels = (
                width,
                height,
                rowstride,
                has_alpha,
                bits,
                channels,
                vec![0u8; 4],
            )
                .to_variant();
            let case =
                format!("{index}: {width} {height} {rowstride} {has_alpha} {bits} {channels}");
            assert_eq!(save_pixels(&path, &pixels).is_some(), is_saved, "{case}");
            assert_eq!(path.exists(), is_saved, "{case}");
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
