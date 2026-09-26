mod migrate;
mod pages;

use std::rc::Rc;

use crate::core::shell;
use crate::panels::settings::Settings;
use crate::platform::ipc::Ipc;
use crate::platform::shortcuts::Shortcuts;

const APP_ID: &str = "quickshell";
const NAME: &str = "Quickshell";

pub fn prepare() {
    shell::set_name(NAME);
    migrate::run();
}

pub fn install(ipc: &Ipc, settings: &Rc<Settings>) {
    if !running()
        && let Some(shortcuts) = Shortcuts::publish(APP_ID)
    {
        std::mem::forget(shortcuts);
    }
    ipc.add_with("settings", "openPage", &["page: string"], {
        let settings = settings.clone();
        move |arguments| settings.open(Some(pages::native(&arguments[0])))
    });
}

fn running() -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let Ok(name) = std::fs::read_to_string(entry.path().join("comm")) else {
            return false;
        };
        name.trim() == "qs" || name.trim() == "quickshell"
    })
}
