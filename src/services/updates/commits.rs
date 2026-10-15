use gtk4::glib;
use serde_json::Value;

use super::{BRANCH, REPOSITORY, Updates};
use crate::core::process;

const VERSION: &str = env!("PROSCENIO_VERSION");

impl Updates {
    pub(super) fn refresh_shell(&self) {
        let Some(commit) = installed_commit(VERSION) else {
            return;
        };
        if self.is_refreshing_shell.replace(true) {
            self.is_shell_refresh_queued.set(true);
            return;
        }
        let url = format!("https://api.github.com/repos/{REPOSITORY}/compare/{commit}...{BRANCH}");
        let updates = self.clone();
        glib::spawn_future_local(async move {
            let body = process::capture_text(process::command(&[
                "curl",
                "-s",
                "--connect-timeout",
                "15",
                "--speed-limit",
                "1",
                "--speed-time",
                "30",
                "--max-time",
                "120",
                "-H",
                "Accept: application/vnd.github+json",
                &url,
            ]))
            .await;
            if let Some((commits_behind, commit_subjects)) =
                body.as_deref().and_then(read_comparison)
            {
                updates.commits_behind.set(commits_behind);
                updates.commit_subjects.replace(commit_subjects);
                updates.listeners.notify();
            }
            updates.is_refreshing_shell.set(false);
            if updates.is_shell_refresh_queued.replace(false) {
                updates.refresh_shell();
            }
        });
    }
}

fn installed_commit(version: &str) -> Option<&str> {
    let (count, hash) = version.strip_prefix('r')?.split_once('.')?;
    let is_numeric = !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit());
    (is_numeric && !hash.is_empty()).then_some(hash)
}

fn read_comparison(body: &str) -> Option<(i32, Vec<String>)> {
    let comparison = serde_json::from_str::<Value>(body).ok()?;
    if comparison.get("message").and_then(Value::as_str) == Some("Not Found") {
        return Some((0, Vec::new()));
    }
    Some((
        read_commits_behind(&comparison)?,
        read_commit_subjects(&comparison),
    ))
}

fn read_commits_behind(comparison: &Value) -> Option<i32> {
    let count = comparison.get("ahead_by")?.as_u64()?;
    Some(i32::try_from(count).unwrap_or(i32::MAX))
}

fn read_commit_subjects(comparison: &Value) -> Vec<String> {
    comparison
        .get("commits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|commit| commit.pointer("/commit/message")?.as_str())
        .map(|message| message.lines().next().unwrap_or_default().to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_commit_is_the_hash_in_the_build_version() {
        assert_eq!(installed_commit("r204.e28f698"), Some("e28f698"));
        assert_eq!(installed_commit("0.1.0"), None);
        assert_eq!(installed_commit("r12."), None);
    }

    #[test]
    fn the_shell_is_as_far_behind_as_main_is_ahead_of_its_commit() {
        let read = |json: &str| read_commits_behind(&serde_json::from_str(json).unwrap());
        assert_eq!(
            read(r#"{"status":"ahead","ahead_by":3,"behind_by":0}"#),
            Some(3)
        );
        assert_eq!(
            read(r#"{"status":"identical","ahead_by":0,"behind_by":0}"#),
            Some(0)
        );
        assert_eq!(
            read(r#"{"status":"diverged","ahead_by":2,"behind_by":5}"#),
            Some(2)
        );
        assert_eq!(read(r#"{"message":"Not Found"}"#), None);
        assert_eq!(read(r#"{"ahead_by":4294967299}"#), Some(i32::MAX));
        assert_eq!(read(r#"{"ahead_by":-5}"#), None);
    }

    #[test]
    fn a_failed_comparison_gives_no_count_and_an_unknown_commit_counts_as_up_to_date() {
        assert_eq!(
            read_comparison(r#"{"ahead_by":1,"commits":[{"commit":{"message":"Bar: clock"}}]}"#),
            Some((1, vec!["Bar: clock".to_owned()]))
        );
        assert_eq!(read_comparison(""), None);
        assert_eq!(
            read_comparison(r#"{"message":"API rate limit exceeded for 127.0.0.1."}"#),
            None
        );
        assert_eq!(
            read_comparison(r#"{"message":"Not Found"}"#),
            Some((0, Vec::new()))
        );
    }

    #[test]
    fn the_commit_subjects_are_the_first_lines_of_the_compared_commit_messages() {
        let comparison = serde_json::from_str(
            r#"{"ahead_by":2,"commits":[
                {"commit":{"message":"Theme: redraw every widget\n\nBody"}},
                {"commit":{"message":"Docs: links"}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            read_commit_subjects(&comparison),
            ["Theme: redraw every widget", "Docs: links"]
        );
        assert!(
            read_commit_subjects(&serde_json::from_str(r#"{"message":"Not Found"}"#).unwrap())
                .is_empty()
        );
    }
}
