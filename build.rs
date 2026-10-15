#[path = "src/panels/settings/scan.rs"]
mod scan;

use std::fmt::Write;
use std::process::Command;

const PAGES: &str = "src/panels/settings/pages";

fn main() {
    println!("cargo:rerun-if-changed=protocols/hyprland-ctm-control-v1.xml");
    println!("cargo:rerun-if-changed=protocols/hyprland-focus-grab-v1.xml");
    println!("cargo:rerun-if-changed=protocols/hyprland-global-shortcuts-v1.xml");
    println!("cargo:rerun-if-changed=protocols/hyprland-lock-notify-v1.xml");
    println!("cargo:rerun-if-changed=protocols/hyprland-toplevel-export-v1.xml");
    println!("cargo:rerun-if-changed=protocols/linux-dmabuf-v1.xml");
    settings_index();
    version();
}

fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn version() {
    if let Some(log) = git(&["rev-parse", "--git-path", "logs/HEAD"]) {
        println!("cargo:rerun-if-changed={log}");
    }
    let version = match (
        git(&["rev-list", "--count", "HEAD"]),
        git(&["rev-parse", "--short", "HEAD"]),
    ) {
        (Some(count), Some(hash)) => format!("r{count}.{hash}"),
        _ => std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
    };
    println!("cargo:rustc-env=PROSCENIO_VERSION={version}");
}

fn settings_index() {
    println!("cargo:rerun-if-changed={PAGES}");
    println!("cargo:rerun-if-changed=src/panels/settings/scan.rs");
    let ids = read_page_ids();
    let mut files = Vec::new();
    collect_page_sources(std::path::Path::new(PAGES), &ids, &mut files);
    files.sort();
    let mut index = String::from("&[\n");
    for (page, path) in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let source = std::fs::read_to_string(&path).expect("a settings page source");
        for setting in scan::settings(&source) {
            let _ = writeln!(
                index,
                "    Setting {{ page: {page:?}, path: &{:?}, title: {:?} }},",
                setting.path, setting.title
            );
        }
    }
    index.push_str("]\n");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join("settings_index.rs"), index).expect("the settings index");
}

fn read_page_ids() -> Vec<String> {
    let source = std::fs::read_to_string(format!("{PAGES}/mod.rs"))
        .expect("the settings pages module source");
    source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("id: \"")?.strip_suffix("\","))
        .map(str::to_owned)
        .collect()
}

fn collect_page_sources(
    directory: &std::path::Path,
    ids: &[String],
    sources: &mut Vec<(String, std::path::PathBuf)>,
) {
    let entries = std::fs::read_dir(directory).expect("a settings pages directory");
    for path in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
        if path.is_dir() {
            collect_page_sources(&path, ids, sources);
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let name = if stem == "mod" {
            directory.file_name().unwrap_or_default().to_string_lossy()
        } else {
            stem
        };
        if ids.iter().any(|id| *id == name) {
            sources.push((name.into_owned(), path));
        }
    }
}
