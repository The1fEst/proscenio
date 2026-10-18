use std::path::Path;

pub fn format_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub fn describe_path_error(path: &Path, error: std::io::Error) -> String {
    format!("{}: {error}", path.display())
}
