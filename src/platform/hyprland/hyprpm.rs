use std::ffi::OsStr;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, symlink};
use std::path::{Path, PathBuf};

use gtk4::glib;
use serde_json::Value;
use toml_edit::DocumentMut;

use crate::core::{paths, process};
use crate::platform::hyprland::hypr;

pub const PLUGIN_NAME: &str = "proscenio_hyprland";
pub const REPOSITORY_URL: &str = "https://github.com/The1fEst/proscenio";
pub const HYPRPM: &str = "/usr/bin/hyprpm";
pub const SYSTEM_TOOLS: &str = "/usr/bin";
const ESCALATORS: [&str; 3] = ["sudo", "doas", "run0"];
const FORWARDER_NAME: &str = "run0";

#[derive(Debug, PartialEq, Eq)]
pub enum PluginAction {
    Load,
    Rebuild { abi_hash: String },
    ReportFailure { abi_hash: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum RebuildOutcome {
    Loadable,
    LoadableAfterLogin,
    Failure,
}

fn cache_dir() -> PathBuf {
    PathBuf::from("/var/cache/hyprpm").join(glib::user_name())
}

pub fn tool_dir() -> PathBuf {
    paths::cache().join("hyprpm-tools")
}

fn has_plugin_flag(state_text: &str, key: &str) -> bool {
    state_text.parse::<DocumentMut>().is_ok_and(|state| {
        state
            .get(PLUGIN_NAME)
            .and_then(|plugin| plugin.get(key))
            .and_then(|item| item.as_bool())
            == Some(true)
    })
}

fn has_repository_url(state_text: &str, url: &str) -> bool {
    state_text.parse::<DocumentMut>().is_ok_and(|state| {
        state
            .get("repository")
            .and_then(|repository| repository.get("url"))
            .and_then(|item| item.as_str())
            == Some(url)
    })
}

fn has_repository_state(predicate: impl Fn(&str) -> bool) -> bool {
    let Ok(repositories) = std::fs::read_dir(cache_dir()) else {
        return false;
    };
    repositories.flatten().any(|repository| {
        std::fs::read_to_string(repository.path().join("state.toml"))
            .is_ok_and(|state_text| predicate(&state_text))
    })
}

fn has_plugin_flag_in_hyprpm(key: &str) -> bool {
    has_repository_state(|state_text| has_plugin_flag(state_text, key))
}

pub fn is_plugin_enabled_in_hyprpm() -> bool {
    has_plugin_flag_in_hyprpm("enabled")
}

pub fn is_repository_in_hyprpm() -> bool {
    has_repository_state(|state_text| has_repository_url(state_text, REPOSITORY_URL))
}

fn is_plugin_loaded() -> bool {
    hypr::option_int(&format!("plugin:{PLUGIN_NAME}:bars:height")).is_some()
}

fn parse_built_abi_hash(state_text: &str) -> Option<String> {
    let state = state_text.parse::<DocumentMut>().ok()?;
    let hash = state.get("state")?.get("hash")?.as_str()?;
    (!hash.is_empty()).then(|| hash.to_owned())
}

fn parse_abi_hash(version: &Value) -> Option<String> {
    let hash = version.get("abiHash")?.as_str()?;
    (!hash.is_empty()).then(|| hash.to_owned())
}

fn built_abi_hash() -> Option<String> {
    if has_plugin_flag_in_hyprpm("failed") {
        return None;
    }
    parse_built_abi_hash(&std::fs::read_to_string(cache_dir().join("state.toml")).ok()?)
}

fn installed_abi_hash() -> Option<String> {
    let text = process::output(&["Hyprland", "--version-json"])?;
    parse_abi_hash(&serde_json::from_str(&text).ok()?)
}

fn running_abi_hash() -> Option<String> {
    parse_abi_hash(&hypr::json("version")?)
}

fn plan_action(
    is_loaded: bool,
    built_hash: Option<&str>,
    installed_hash: Option<&str>,
    running_hash: Option<&str>,
    attempted_hash: Option<&str>,
) -> Option<PluginAction> {
    if !is_loaded && built_hash.is_some() && built_hash == running_hash {
        return Some(PluginAction::Load);
    }
    if let Some(installed_hash) = installed_hash
        && built_hash != Some(installed_hash)
    {
        let abi_hash = installed_hash.to_owned();
        return Some(if attempted_hash == Some(installed_hash) {
            PluginAction::ReportFailure { abi_hash }
        } else {
            PluginAction::Rebuild { abi_hash }
        });
    }
    (!is_loaded && running_hash.is_none()).then_some(PluginAction::Load)
}

pub fn check_plugin(attempted_hash: Option<&str>) -> Option<PluginAction> {
    if !is_plugin_enabled_in_hyprpm() {
        return None;
    }
    plan_action(
        is_plugin_loaded(),
        built_abi_hash().as_deref(),
        installed_abi_hash().as_deref(),
        running_abi_hash().as_deref(),
        attempted_hash,
    )
}

fn judge_rebuild(
    is_enabled: bool,
    built_hash: Option<&str>,
    installed_hash: Option<&str>,
    running_hash: Option<&str>,
) -> RebuildOutcome {
    match (built_hash, installed_hash) {
        (Some(built_hash), Some(installed_hash)) if is_enabled && built_hash == installed_hash => {
            if running_hash == Some(installed_hash) {
                RebuildOutcome::Loadable
            } else {
                RebuildOutcome::LoadableAfterLogin
            }
        }
        _ => RebuildOutcome::Failure,
    }
}

pub fn check_rebuild() -> RebuildOutcome {
    judge_rebuild(
        is_plugin_enabled_in_hyprpm(),
        built_abi_hash().as_deref(),
        installed_abi_hash().as_deref(),
        running_abi_hash().as_deref(),
    )
}

pub fn expected_runtime_dir() -> Option<String> {
    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    Some(format!("/run/user/{uid}"))
}

fn is_tool_allowed(name: &OsStr) -> bool {
    !ESCALATORS.iter().any(|escalator| name == *escalator)
}

pub fn prepare_tool_dir(source: &Path, target: &Path, forwarder: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(target)?,
        Ok(_) => std::fs::remove_file(target)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::DirBuilder::new().mode(0o700).create(target)?;
    for entry in std::fs::read_dir(source)? {
        let name = entry?.file_name();
        if is_tool_allowed(&name) {
            symlink(source.join(&name), target.join(&name))?;
        }
    }
    symlink(forwarder, target.join(FORWARDER_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_plugin_counts_as_enabled_only_when_its_table_says_so() {
        let cases = [
            (
                "enabled",
                "[proscenio_hyprland]\nenabled = true\nfailed = false\nfilename = 'proscenio_hyprland.so'\n\n[repository]\nname = 'proscenio'\n",
                true,
            ),
            ("disabled", "[proscenio_hyprland]\nenabled = false\n", false),
            ("another plugin", "[hyprbars]\nenabled = true\n", false),
            (
                "no enabled key",
                "[proscenio_hyprland]\nfailed = false\n",
                false,
            ),
            (
                "a string, not a boolean",
                "[proscenio_hyprland]\nenabled = 'true'\n",
                false,
            ),
            ("not TOML", "[proscenio_hyprland\nenabled = true\n", false),
            ("empty", "", false),
        ];
        for (name, state_text, expected) in cases {
            assert_eq!(has_plugin_flag(state_text, "enabled"), expected, "{name}");
        }
        assert!(has_plugin_flag(
            "[proscenio_hyprland]\nenabled = true\nfailed = true\n",
            "failed"
        ));
    }

    #[test]
    fn the_repository_counts_as_added_only_with_its_own_url() {
        let cases = [
            (
                "added",
                "[proscenio_hyprland]\nenabled = false\n\n[repository]\nname = 'proscenio'\nurl = 'https://github.com/The1fEst/proscenio'\n",
                true,
            ),
            (
                "another url",
                "[repository]\nname = 'proscenio'\nurl = 'https://github.com/someone/proscenio'\n",
                false,
            ),
            (
                "the url with .git",
                "[repository]\nurl = 'https://github.com/The1fEst/proscenio.git'\n",
                false,
            ),
            (
                "the url outside the repository table",
                "url = 'https://github.com/The1fEst/proscenio'\n",
                false,
            ),
            ("no url", "[repository]\nname = 'proscenio'\n", false),
            (
                "not TOML",
                "[repository\nurl = 'https://github.com/The1fEst/proscenio'\n",
                false,
            ),
        ];
        for (name, state_text, expected) in cases {
            assert_eq!(
                has_repository_url(state_text, REPOSITORY_URL),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn the_built_abi_hash_comes_from_the_global_state_table() {
        let cases = [
            (
                "built",
                "[state]\ndont_warn_install = true\nhash = 'abc_aq_0.16'\n",
                Some("abc_aq_0.16"),
            ),
            ("never built", "[state]\ndont_warn_install = true\n", None),
            ("empty hash", "[state]\nhash = ''\n", None),
            ("a number", "[state]\nhash = 5\n", None),
            ("not TOML", "[state\nhash = 'abc'\n", None),
        ];
        for (name, state_text, expected) in cases {
            assert_eq!(
                parse_built_abi_hash(state_text).as_deref(),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn the_abi_hash_comes_from_the_version_reply() {
        assert_eq!(
            parse_abi_hash(&json!({ "commit": "abc", "abiHash": "abc_aq_0.16" })).as_deref(),
            Some("abc_aq_0.16")
        );
        assert_eq!(parse_abi_hash(&json!({ "commit": "abc" })), None);
        assert_eq!(parse_abi_hash(&json!({ "abiHash": "" })), None);
    }

    #[test]
    fn a_plugin_built_for_another_hyprland_is_rebuilt_once_per_version() {
        let rebuild_action = || {
            Some(PluginAction::Rebuild {
                abi_hash: "new".to_owned(),
            })
        };
        let new_hash = Some("new");
        let old_hash = Some("old");
        let cases = [
            (
                "current and loaded",
                true,
                new_hash,
                new_hash,
                new_hash,
                None,
                None,
            ),
            (
                "current, not loaded",
                false,
                new_hash,
                new_hash,
                new_hash,
                None,
                Some(PluginAction::Load),
            ),
            (
                "outdated",
                false,
                old_hash,
                new_hash,
                new_hash,
                None,
                rebuild_action(),
            ),
            (
                "outdated after an update, still loaded",
                true,
                old_hash,
                new_hash,
                old_hash,
                None,
                rebuild_action(),
            ),
            (
                "never built",
                false,
                None,
                new_hash,
                new_hash,
                None,
                rebuild_action(),
            ),
            (
                "an older attempt",
                false,
                old_hash,
                new_hash,
                new_hash,
                old_hash,
                rebuild_action(),
            ),
            (
                "attempted for this version",
                false,
                old_hash,
                new_hash,
                new_hash,
                new_hash,
                Some(PluginAction::ReportFailure {
                    abi_hash: "new".to_owned(),
                }),
            ),
            (
                "attempted, the session runs the version built for",
                false,
                old_hash,
                new_hash,
                old_hash,
                new_hash,
                Some(PluginAction::Load),
            ),
            (
                "rebuilt for the next login, not loaded",
                false,
                new_hash,
                new_hash,
                old_hash,
                None,
                None,
            ),
            (
                "installed version unknown",
                false,
                old_hash,
                None,
                None,
                None,
                Some(PluginAction::Load),
            ),
        ];
        for (name, is_loaded, built_hash, installed_hash, running_hash, attempted_hash, expected) in
            cases
        {
            assert_eq!(
                plan_action(
                    is_loaded,
                    built_hash,
                    installed_hash,
                    running_hash,
                    attempted_hash
                ),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn a_rebuild_loads_only_an_enabled_plugin_and_now_only_on_the_installed_hyprland() {
        let cases = [
            (
                true,
                Some("new"),
                Some("new"),
                Some("new"),
                RebuildOutcome::Loadable,
            ),
            (
                true,
                Some("new"),
                Some("new"),
                Some("old"),
                RebuildOutcome::LoadableAfterLogin,
            ),
            (
                false,
                Some("new"),
                Some("new"),
                Some("new"),
                RebuildOutcome::Failure,
            ),
            (
                true,
                Some("old"),
                Some("new"),
                Some("new"),
                RebuildOutcome::Failure,
            ),
            (
                true,
                None,
                Some("new"),
                Some("new"),
                RebuildOutcome::Failure,
            ),
            (
                true,
                Some("new"),
                None,
                Some("new"),
                RebuildOutcome::Failure,
            ),
        ];
        for (is_enabled, built_hash, installed_hash, running_hash, expected) in cases {
            assert_eq!(
                judge_rebuild(is_enabled, built_hash, installed_hash, running_hash),
                expected,
                "{is_enabled} {built_hash:?} {installed_hash:?} {running_hash:?}"
            );
        }
    }

    #[test]
    fn the_tool_dir_links_every_tool_but_the_escalators_and_run0_to_the_forwarder() {
        let root = std::env::temp_dir().join(format!("proscenio-tools-{}", std::process::id()));
        let source = root.join("bin");
        let target = root.join("cache").join("tools");
        let forwarder = root.join("helper");
        std::fs::create_dir_all(&source).unwrap();
        for name in ["git", "cmake", "run0", "sudo", "doas", "sudoedit"] {
            std::fs::write(source.join(name), "").unwrap();
        }
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("stale"), "").unwrap();

        prepare_tool_dir(&source, &target, &forwarder).unwrap();

        let mut names: Vec<String> = std::fs::read_dir(&target)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["cmake", "git", "run0", "sudoedit"]);
        assert_eq!(
            std::fs::read_link(target.join("git")).unwrap(),
            source.join("git")
        );
        assert_eq!(std::fs::read_link(target.join("run0")).unwrap(), forwarder);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_tool_dir_replaces_a_symlink_without_following_it() {
        let root =
            std::env::temp_dir().join(format!("proscenio-tools-link-{}", std::process::id()));
        let source = root.join("bin");
        let elsewhere = root.join("elsewhere");
        let target = root.join("tools");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("kept"), "").unwrap();
        symlink(&elsewhere, &target).unwrap();

        prepare_tool_dir(&source, &target, &root.join("helper")).unwrap();

        assert!(elsewhere.join("kept").exists());
        assert!(std::fs::symlink_metadata(&target).unwrap().is_dir());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
