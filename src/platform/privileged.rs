use crate::core::i18n::tr;
use crate::core::process;

pub async fn request(command: &str, words: Vec<String>, failed: String) -> Result<(), String> {
    let mut line = process::command(&["pkexec", &process::executable(), command]);
    line.args(&words).env("LANG", "C");
    let Some(finished) = process::capture(line).await else {
        return Err(tr("Could not run pkexec"));
    };
    if finished.status.success() {
        return Ok(());
    }
    let errors = String::from_utf8_lossy(&finished.stderr);
    Err(errors
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start_matches("ERROR: ").to_owned())
        .unwrap_or(failed))
}
