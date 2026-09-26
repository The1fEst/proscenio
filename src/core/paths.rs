use gtk4::glib;
use std::path::PathBuf;

const NAME: &str = "proscenio";

pub fn config() -> PathBuf {
    glib::user_config_dir().join(NAME)
}

pub fn state() -> PathBuf {
    state_home().join(NAME)
}

pub fn cache() -> PathBuf {
    glib::user_cache_dir().join(NAME)
}

pub fn runtime() -> PathBuf {
    glib::user_runtime_dir().join(NAME)
}

pub fn generated() -> PathBuf {
    state().join("generated")
}

pub fn state_home() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| glib::home_dir().join(".local/state"))
}

pub fn prepare() {
    let _ = std::fs::create_dir_all(generated());
}

pub fn expand_home(path: &str) -> String {
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            format!("{}{rest}", glib::home_dir().display())
        }
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_leading_tilde_of_the_own_home_expands() {
        let home = glib::home_dir().display().to_string();
        assert_eq!(expand_home("~"), home);
        assert_eq!(
            expand_home("~/Pictures/Shots"),
            format!("{home}/Pictures/Shots")
        );
        assert_eq!(expand_home("~other/x"), "~other/x");
        assert_eq!(expand_home("/tmp/~/x"), "/tmp/~/x");
        assert_eq!(expand_home(""), "");
    }
}
