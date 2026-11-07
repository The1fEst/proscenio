use std::path::PathBuf;

use gtk4::glib;
use toml_edit::DocumentMut;

use crate::core::process;
use crate::platform::hyprland::hypr;

const PLUGIN_NAME: &str = "proscenio_bars";

fn cache_dir() -> PathBuf {
    PathBuf::from("/var/cache/hyprpm").join(glib::user_name())
}

fn is_plugin_enabled(state_text: &str) -> bool {
    state_text.parse::<DocumentMut>().is_ok_and(|state| {
        state
            .get(PLUGIN_NAME)
            .and_then(|plugin| plugin.get("enabled"))
            .and_then(|item| item.as_bool())
            == Some(true)
    })
}

fn is_plugin_enabled_in_hyprpm() -> bool {
    let Ok(repositories) = std::fs::read_dir(cache_dir()) else {
        return false;
    };
    repositories.flatten().any(|repository| {
        std::fs::read_to_string(repository.path().join("state.toml"))
            .is_ok_and(|state_text| is_plugin_enabled(&state_text))
    })
}

fn is_plugin_loaded() -> bool {
    hypr::option_int(&format!("plugin:{PLUGIN_NAME}:bar_height")).is_some()
}

pub fn load_enabled_plugin() {
    if is_plugin_enabled_in_hyprpm() && !is_plugin_loaded() {
        process::start(process::quiet(&["hyprpm", "reload"]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_counts_as_enabled_only_when_its_table_says_so() {
        let cases = [
            (
                "enabled",
                "[proscenio_bars]\nenabled = true\nfailed = false\nfilename = 'proscenio_bars.so'\n\n[repository]\nname = 'proscenio'\n",
                true,
            ),
            ("disabled", "[proscenio_bars]\nenabled = false\n", false),
            ("another plugin", "[hyprbars]\nenabled = true\n", false),
            (
                "no enabled key",
                "[proscenio_bars]\nfailed = false\n",
                false,
            ),
            (
                "a string, not a boolean",
                "[proscenio_bars]\nenabled = 'true'\n",
                false,
            ),
            ("not TOML", "[proscenio_bars\nenabled = true\n", false),
            ("empty", "", false),
        ];
        for (name, state_text, expected) in cases {
            assert_eq!(is_plugin_enabled(state_text), expected, "{name}");
        }
    }
}
