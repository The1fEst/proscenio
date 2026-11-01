use gtk4::glib::{self, Variant};
use std::cell::Cell;
use std::path::PathBuf;

use super::images::{file_uri_to_path, is_file_uri};
use super::{Notifications, URGENCY_CRITICAL, hint, hint_string};
use crate::core::{config, process};
use crate::services::media::soundtheme;

const DEFAULT_SOUND: &str = "message-new-instant";
const CRITICAL_SOUND: &str = "dialog-warning";
const BURST_WINDOW_MILLIS: i64 = 1000;

#[derive(Debug, Default, PartialEq)]
pub(super) struct SoundHints {
    is_suppressed: bool,
    file: Option<PathBuf>,
    name: Option<String>,
}

#[derive(Debug, PartialEq)]
pub(super) enum Sound {
    File(PathBuf),
    Name(String),
    Default,
}

#[derive(Default)]
pub(super) struct BurstLimiter {
    last_millis: Cell<Option<i64>>,
}

impl BurstLimiter {
    fn allow(&self, now_millis: i64) -> bool {
        let is_burst = self
            .last_millis
            .get()
            .is_some_and(|last| (0..BURST_WINDOW_MILLIS).contains(&(now_millis - last)));
        if !is_burst {
            self.last_millis.set(Some(now_millis));
        }
        !is_burst
    }
}

impl SoundHints {
    fn read(hints: &Variant) -> Self {
        SoundHints {
            is_suppressed: hint(hints, "suppress-sound")
                .and_then(|value| value.get::<bool>())
                .unwrap_or(false),
            file: hint_string(hints, "sound-file").and_then(|location| locate_file(&location)),
            name: hint_string(hints, "sound-name").filter(|name| !name.is_empty()),
        }
    }
}

fn locate_file(location: &str) -> Option<PathBuf> {
    let path = if is_file_uri(location) {
        file_uri_to_path(location)?
    } else {
        PathBuf::from(location)
    };
    (path.is_absolute() && path.is_file()).then_some(path)
}

fn choose_sound(hints: SoundHints, is_enabled: bool) -> Option<Sound> {
    if !is_enabled || hints.is_suppressed {
        return None;
    }
    if let Some(file) = hints.file {
        return Some(Sound::File(file));
    }
    if let Some(name) = hints.name {
        return Some(Sound::Name(name));
    }
    Some(Sound::Default)
}

fn locate_sound(sound: Sound, urgency: u8, theme: &str, bases: &[PathBuf]) -> Option<PathBuf> {
    match sound {
        Sound::File(path) => Some(path),
        Sound::Name(name) => soundtheme::find_sound(bases, theme, &name),
        Sound::Default => (urgency == URGENCY_CRITICAL)
            .then(|| soundtheme::find_sound(bases, theme, CRITICAL_SOUND))
            .flatten()
            .or_else(|| soundtheme::find_sound(bases, theme, DEFAULT_SOUND)),
    }
}

impl Notifications {
    pub(super) fn play_sound(&self, hints: &Variant, urgency: u8) {
        let config = config::current();
        let Some(sound) = choose_sound(SoundHints::read(hints), config.sounds_notifications) else {
            return;
        };
        let bases = soundtheme::data_dirs();
        let Some(path) = locate_sound(sound, urgency, &config.sounds_theme, &bases) else {
            return;
        };
        let Some(path) = path.to_str() else {
            return;
        };
        if self.sound_limiter.allow(glib::monotonic_time() / 1000) {
            process::detach(&["paplay", path]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::*;
    use std::collections::HashMap;

    fn read_hints(entries: &[(&str, Variant)]) -> SoundHints {
        let hints: HashMap<String, Variant> = entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect();
        SoundHints::read(&hints.to_variant())
    }

    fn touch(folder: &std::path::Path, relative: &str) -> PathBuf {
        let path = folder.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"sound").unwrap();
        path
    }

    #[test]
    fn the_sound_hints_choose_the_sound_and_the_global_option_gates_every_sound() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-sound-hints-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let file = touch(&folder, "ping.oga");
        let file_text = file.to_str().unwrap();
        let uri = format!("file://{file_text}");
        let missing = folder.join("absent.oga");
        let name_hint = || ("sound-name", "bell".to_variant());
        let suppress_hint = || ("suppress-sound", true.to_variant());
        let file_hint = || ("sound-file", file_text.to_variant());
        let cases = [
            (vec![], true, Some(Sound::Default)),
            (vec![], false, None),
            (vec![suppress_hint()], true, None),
            (vec![suppress_hint(), name_hint(), file_hint()], true, None),
            (
                vec![("suppress-sound", false.to_variant())],
                true,
                Some(Sound::Default),
            ),
            (vec![name_hint()], false, None),
            (
                vec![name_hint()],
                true,
                Some(Sound::Name("bell".to_owned())),
            ),
            (vec![file_hint()], true, Some(Sound::File(file.clone()))),
            (vec![file_hint()], false, None),
            (
                vec![file_hint(), name_hint()],
                true,
                Some(Sound::File(file.clone())),
            ),
            (vec![file_hint(), name_hint()], false, None),
            (
                vec![("sound-file", uri.to_variant())],
                true,
                Some(Sound::File(file.clone())),
            ),
            (
                vec![("sound-file", "ping.oga".to_variant())],
                true,
                Some(Sound::Default),
            ),
            (
                vec![
                    ("sound-file", missing.to_str().unwrap().to_variant()),
                    name_hint(),
                ],
                true,
                Some(Sound::Name("bell".to_owned())),
            ),
            (
                vec![("sound-name", "".to_variant())],
                true,
                Some(Sound::Default),
            ),
        ];
        for (index, (entries, is_enabled, expected)) in cases.into_iter().enumerate() {
            assert_eq!(
                choose_sound(read_hints(&entries), is_enabled),
                expected,
                "case {index}"
            );
        }
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_default_sound_is_the_themes_own_and_a_critical_one_has_its_own() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-sound-default-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let plain = touch(&folder, "plain/stereo/message-new-instant.oga");
        let warning = touch(&folder, "alert/stereo/dialog-warning.oga");
        let alert_plain = touch(&folder, "alert/stereo/message-new-instant.oga");
        let bell = touch(&folder, "alert/stereo/bell.oga");
        let bases = [folder.clone()];
        let locate = |sound, urgency, theme| locate_sound(sound, urgency, theme, &bases);
        assert_eq!(locate(Sound::Default, 1, "plain"), Some(plain.clone()));
        assert_eq!(locate(Sound::Default, 0, "alert"), Some(alert_plain));
        assert_eq!(locate(Sound::Default, 2, "alert"), Some(warning));
        assert_eq!(locate(Sound::Default, 2, "plain"), Some(plain));
        assert_eq!(
            locate(Sound::Name("bell".to_owned()), 2, "alert"),
            Some(bell)
        );
        assert_eq!(locate(Sound::Name("absent".to_owned()), 1, "alert"), None);
        assert_eq!(locate(Sound::Default, 1, "empty"), None);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_burst_plays_one_sound_per_window() {
        let limiter = BurstLimiter::default();
        let cases = [
            (5000, true),
            (5000, false),
            (5999, false),
            (6000, true),
            (6500, false),
            (7000, true),
            (4000, true),
        ];
        for (now_millis, expected) in cases {
            assert_eq!(limiter.allow(now_millis), expected, "{now_millis}");
        }
    }
}
