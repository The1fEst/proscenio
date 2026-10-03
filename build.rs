#[path = "src/panels/settings/scan.rs"]
mod scan;

use std::fmt::Write;
use std::process::Command;

const PAGES: &str = "src/panels/settings/pages";

fn main() {
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
    let mut files: Vec<_> = std::fs::read_dir(PAGES)
        .expect("the settings pages directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .filter(|path| path.file_stem().is_some_and(|stem| stem != "mod"))
        .collect();
    files.sort();
    let mut index = String::from("&[\n");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let page = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let source = std::fs::read_to_string(&path).expect("a settings page source");
        for found in scan::settings(&source) {
            let _ = writeln!(
                index,
                "    Setting {{ page: {page:?}, path: &{:?}, title: {:?} }},",
                found.path, found.title
            );
        }
    }
    index.push_str("]\n");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join("settings_index.rs"), index).expect("the settings index");
}
