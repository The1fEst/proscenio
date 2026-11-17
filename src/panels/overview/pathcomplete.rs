use std::path::{Path, PathBuf};

use crate::core::paths::expand_home;

const SCAN_LIMIT: usize = 5000;
const FILE_SCHEME: &str = "file://";

pub struct PathEntry {
    pub name: String,
    pub is_dir: bool,
}

impl PathEntry {
    pub fn to_completion(&self, base: &str) -> String {
        let suffix = if self.is_dir { "/" } else { "" };
        format!("{base}{}{suffix}", escape(&self.name))
    }
}

pub struct PathCandidates {
    pub base: String,
    pub entries: Vec<PathEntry>,
}

pub fn find_candidates(text: &str) -> Option<PathCandidates> {
    let (head, token) = split_path(text)?;
    if is_quoted(token) {
        return None;
    }
    Some(read_candidates(head, token))
}

pub fn complete_path(text: &str) -> Option<String> {
    if &text[token_start(text)..] == "~" {
        return Some(format!("{text}/"));
    }
    let (head, token) = split_path(text)?;
    if is_quoted(token) {
        return Some(text.to_owned());
    }
    let candidates = read_candidates(head, token);
    let names: Vec<&str> = candidates
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    let Some(common) = common_prefix(&names) else {
        return Some(text.to_owned());
    };
    let suffix = match candidates.entries.as_slice() {
        [entry] if entry.is_dir => "/",
        _ => "",
    };
    Some(format!("{}{}{suffix}", candidates.base, escape(&common)))
}

fn split_path(text: &str) -> Option<(&str, &str)> {
    let token = &text[token_start(text)..];
    let token = token.strip_prefix(FILE_SCHEME).unwrap_or(token);
    token
        .contains('/')
        .then(|| (&text[..text.len() - token.len()], token))
}

fn is_quoted(token: &str) -> bool {
    token.contains(['\'', '"'])
}

fn read_candidates(head: &str, token: &str) -> PathCandidates {
    let (typed_dir, typed_name) = token.split_at(last_slash(token) + 1);
    let directory = PathBuf::from(expand_home(&unescape(typed_dir)));
    PathCandidates {
        base: format!("{head}{typed_dir}"),
        entries: list_matches(&directory, &unescape(typed_name)),
    }
}

fn token_start(text: &str) -> usize {
    let mut start = 0;
    let mut is_escaped = false;
    for (index, character) in text.char_indices() {
        if is_escaped {
            is_escaped = false;
        } else if character == '\\' {
            is_escaped = true;
        } else if character.is_whitespace() {
            start = index + character.len_utf8();
        }
    }
    start
}

fn last_slash(token: &str) -> usize {
    let mut slash = 0;
    let mut is_escaped = false;
    for (index, character) in token.char_indices() {
        if is_escaped {
            is_escaped = false;
        } else if character == '\\' {
            is_escaped = true;
        } else if character == '/' {
            slash = index;
        }
    }
    slash
}

fn unescape(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut is_escaped = false;
    for character in text.chars() {
        if !is_escaped && character == '\\' {
            is_escaped = true;
            continue;
        }
        is_escaped = false;
        result.push(character);
    }
    result
}

fn escape(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    for character in name.chars() {
        if !(character.is_alphanumeric() || "_-.,+@%:=".contains(character)) {
            result.push('\\');
        }
        result.push(character);
    }
    result
}

fn list_matches(directory: &Path, prefix: &str) -> Vec<PathEntry> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let wants_hidden = prefix.starts_with('.');
    let mut matches: Vec<PathEntry> = entries
        .take(SCAN_LIMIT)
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            if name.chars().any(char::is_control)
                || !name.starts_with(prefix)
                || (!wants_hidden && name.starts_with('.'))
            {
                return None;
            }
            Some(PathEntry {
                is_dir: entry.path().is_dir(),
                name,
            })
        })
        .collect();
    matches.sort_by_cached_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
    matches
}

fn common_prefix(names: &[&str]) -> Option<String> {
    let first = *names.first()?;
    let mut length = first.len();
    for name in &names[1..] {
        let shared = first
            .chars()
            .zip(name.chars())
            .take_while(|(left, right)| left == right)
            .map(|(left, _)| left.len_utf8())
            .sum::<usize>();
        length = length.min(shared);
    }
    Some(first[..length].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("proscenio-pathcomplete-{name}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("alpha")).unwrap();
        fs::create_dir_all(root.join("alps")).unwrap();
        fs::create_dir_all(root.join("beta dir")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join("file.txt"), "").unwrap();
        root
    }

    #[test]
    fn completes_paths_like_a_shell() {
        let root = make_tree("shell");
        let base = root.display().to_string();
        let cases = [
            (
                format!("code {base}/fi"),
                Some(format!("code {base}/file.txt")),
            ),
            (
                format!("code {base}/be"),
                Some(format!("code {base}/beta\\ dir/")),
            ),
            (format!("{base}/al"), Some(format!("{base}/alp"))),
            (format!("{base}/alph"), Some(format!("{base}/alpha/"))),
            (format!("{base}/zz"), Some(format!("{base}/zz"))),
            (format!("{base}/h"), Some(format!("{base}/h"))),
            (format!("{base}/."), Some(format!("{base}/.hidden/"))),
            (
                format!("file://{base}/fi"),
                Some(format!("file://{base}/file.txt")),
            ),
            (format!("ls \"{base}/fi"), Some(format!("ls \"{base}/fi"))),
            ("code".to_owned(), None),
            ("code x".to_owned(), None),
        ];
        for (text, expected) in cases {
            assert_eq!(complete_path(&text), expected, "{text}");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn lists_directories_first_and_hides_dotfiles() {
        let root = make_tree("listing");
        fs::create_dir_all(root.join("Zeta")).unwrap();
        fs::write(root.join("Apple.txt"), "").unwrap();
        let base = format!("{}/", root.display());
        let names = |text: &str| -> Vec<String> {
            find_candidates(text)
                .unwrap()
                .entries
                .into_iter()
                .map(|entry| entry.to_completion(""))
                .collect()
        };
        assert_eq!(
            names(&format!("run {base}")),
            [
                "alpha/",
                "alps/",
                "beta\\ dir/",
                "Zeta/",
                "Apple.txt",
                "file.txt"
            ]
        );
        assert_eq!(names(&format!("run {base}.")), [".hidden/"]);
        assert_eq!(names(&format!("run {base}Z")), ["Zeta/"]);
        assert!(find_candidates("run plain").is_none());
        assert!(find_candidates(&format!("run \"{base}")).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn completes_a_token_after_an_escaped_space() {
        let root = make_tree("escaped");
        let base = root.display().to_string().replace(' ', "\\ ");
        let text = format!("{base}/beta\\ d");
        assert_eq!(complete_path(&text), Some(format!("{base}/beta\\ dir/")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn escapes_shell_metacharacters_and_round_trips() {
        let names = [
            "x$(touch pwned)",
            "a;b",
            "a&b",
            "a|b",
            "a`b",
            "a'b",
            "a\"b",
            "a(b)",
            "a*b",
            "a?b",
            "a[b]",
            "a<b",
            "a>b",
            "a#b",
            "#a",
            "a!b",
            "a{b}",
            "a~b",
            "~a",
            "-a",
            "a\\b",
            "a b",
            "a\\ b",
            "ünï cödé",
            "plain-name_1.txt",
        ];
        assert_eq!(escape("x$(touch pwned)"), "x\\$\\(touch\\ pwned\\)");
        assert_eq!(escape("plain-name_1.txt"), "plain-name_1.txt");
        for name in names {
            let escaped = escape(name);
            assert_eq!(unescape(&escaped), name, "{name}");
            let Ok(output) = std::process::Command::new("sh")
                .args(["-c", &format!("printf %s {escaped}")])
                .output()
            else {
                continue;
            };
            assert_eq!(String::from_utf8_lossy(&output.stdout), name, "{name}");
        }
    }

    #[test]
    fn skips_names_with_control_characters() {
        let root = make_tree("control");
        fs::write(root.join("new\nline"), "").unwrap();
        let candidates = find_candidates(&format!("ls {}/", root.display())).unwrap();
        assert!(
            candidates
                .entries
                .iter()
                .all(|entry| entry.name != "new\nline")
        );
        assert_eq!(candidates.entries.len(), 4);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn expands_the_home_directory_but_keeps_the_tilde() {
        assert_eq!(complete_path("cd ~"), Some("cd ~/".to_owned()));
    }
}
