use crate::core::i18n::tr;
use crate::core::process;
use std::process::Command;

pub async fn request(
    command: &str,
    words: Vec<String>,
    fallback_error: String,
) -> Result<(), String> {
    let mut pkexec = process::command(&["pkexec", &process::executable(), command]);
    pkexec.args(&words);
    execute(pkexec, fallback_error).await
}

pub async fn execute(mut pkexec: Command, fallback_error: String) -> Result<(), String> {
    pkexec.env("LANG", "C");
    let Some(output) = process::capture(pkexec).await else {
        return Err(tr("Could not run pkexec"));
    };
    if output.status.success() {
        return Ok(());
    }
    let errors = String::from_utf8_lossy(&output.stderr);
    Err(errors
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start_matches("ERROR: ").to_owned())
        .unwrap_or(fallback_error))
}
