use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use super::desktop;

const REFRESH_DELAY: Duration = Duration::from_millis(200);
const OWNER_ACCESS: u32 = 0o700;

pub struct Watcher {
    state: Rc<WatchState>,
}

struct WatchState {
    trash: PathBuf,
    watched_path: RefCell<PathBuf>,
    monitor: RefCell<Option<gio::FileMonitor>>,
    timer: Cell<Option<glib::SourceId>>,
    on_count: Rc<dyn Fn(usize)>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        if let Some(timer) = self.state.timer.take() {
            timer.remove();
        }
    }
}

impl WatchState {
    fn arm(self: &Rc<Self>) -> bool {
        let mut has_moved = false;
        loop {
            let target = choose_watch_path(&self.trash, Path::is_dir);
            if *self.watched_path.borrow() == target {
                return has_moved;
            }
            let monitor = gio::File::for_path(&target)
                .monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
                .ok();
            if let Some(monitor) = &monitor {
                monitor.connect_changed({
                    let state = Rc::downgrade(self);
                    move |_, _, _, _| {
                        if let Some(state) = state.upgrade() {
                            state.schedule();
                        }
                    }
                });
            }
            self.watched_path.replace(target);
            self.monitor.replace(monitor);
            has_moved = true;
        }
    }

    fn schedule(self: &Rc<Self>) {
        if let Some(pending) = self.timer.take() {
            pending.remove();
        }
        let state = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(REFRESH_DELAY, move || {
            let Some(state) = state.upgrade() else {
                return;
            };
            state.timer.take();
            let has_moved = state.arm();
            if has_moved || *state.watched_path.borrow() == state.trash.join("files") {
                state.report_count();
            }
        });
        self.timer.set(Some(source));
    }

    fn report_count(&self) {
        let trash = self.trash.clone();
        let on_count = self.on_count.clone();
        glib::spawn_future_local(async move {
            if let Ok(item_count) = gio::spawn_blocking(move || count_items(&trash)).await {
                on_count(item_count);
            }
        });
    }
}

pub fn choose_watch_path(trash: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let files = trash.join("files");
    if exists(&files) {
        files
    } else if exists(trash) {
        trash.to_owned()
    } else {
        trash.parent().unwrap_or(trash).to_owned()
    }
}

pub fn directory() -> PathBuf {
    glib::user_data_dir().join("Trash")
}

pub fn choose_icon(item_count: usize) -> &'static str {
    if item_count == 0 {
        "user-trash"
    } else {
        "user-trash-full"
    }
}

pub fn count_items(trash: &Path) -> usize {
    std::fs::read_dir(trash.join("files")).map_or(0, |listing| listing.count())
}

pub fn empty(trash: &Path) -> io::Result<()> {
    let mut failure = None;
    for name in ["files", "info"] {
        let Ok(listing) = std::fs::read_dir(trash.join(name)) else {
            continue;
        };
        for item in listing {
            let result = item.and_then(|item| remove_entry(&item.path()));
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
    }
    match std::fs::remove_file(trash.join("directorysizes")) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            failure.get_or_insert(error);
        }
        _ => {}
    }
    failure.map_or(Ok(()), Err)
}

fn remove_entry(path: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return std::fs::remove_file(path);
    }
    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | OWNER_ACCESS);
    std::fs::set_permissions(path, permissions)?;
    for child in std::fs::read_dir(path)? {
        remove_entry(&child?.path())?;
    }
    std::fs::remove_dir(path)
}

pub fn watch(on_count: impl Fn(usize) + 'static) -> Watcher {
    let state = Rc::new(WatchState {
        trash: directory(),
        watched_path: RefCell::default(),
        monitor: RefCell::default(),
        timer: Cell::default(),
        on_count: Rc::new(on_count),
    });
    state.arm();
    state.report_count();
    Watcher { state }
}

pub fn open() {
    let Some(info) = gio::AppInfo::default_for_type("inode/directory", false) else {
        return;
    };
    let trash = [gio::File::for_uri("trash:///")];
    let entry = info
        .id()
        .and_then(|id| desktop::find(id.strip_suffix(".desktop").unwrap_or(&id)));
    match entry {
        Some(entry) => desktop::launch_with(&entry, &trash),
        None => {
            let _ = info.launch(&trash, gio::AppLaunchContext::NONE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Root(PathBuf);

    impl Root {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("proscenio-trash-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("files")).unwrap();
            std::fs::create_dir_all(root.join("info")).unwrap();
            Root(root)
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_icon_is_full_once_the_trash_holds_an_item() {
        assert_eq!(
            [0, 1, 40].map(choose_icon),
            ["user-trash", "user-trash-full", "user-trash-full"]
        );
    }

    #[test]
    fn the_nearest_existing_ancestor_of_the_files_directory_is_watched() {
        let trash = Path::new("/data/Trash");
        let choose = |existing: &[&str]| {
            choose_watch_path(trash, |path| {
                existing.iter().any(|existing| Path::new(existing) == path)
            })
        };
        assert_eq!(
            [
                choose(&["/data/Trash/files", "/data/Trash"]),
                choose(&["/data/Trash"]),
                choose(&[]),
            ],
            [
                PathBuf::from("/data/Trash/files"),
                PathBuf::from("/data/Trash"),
                PathBuf::from("/data"),
            ]
        );
    }

    #[test]
    fn items_are_counted_in_the_files_directory_and_a_missing_trash_is_empty() {
        let root = Root::new("count");
        assert_eq!(count_items(&root.0), 0);
        std::fs::write(root.0.join("files/a.txt"), "a").unwrap();
        std::fs::create_dir(root.0.join("files/folder")).unwrap();
        std::fs::write(root.0.join("info/a.txt.trashinfo"), "[Trash Info]").unwrap();
        assert_eq!(count_items(&root.0), 2);
        assert_eq!(count_items(&root.0.join("missing")), 0);
    }

    #[test]
    fn emptying_deletes_every_item_even_in_a_read_only_folder_and_keeps_the_trash() {
        let root = Root::new("empty");
        let folder = root.0.join("files/folder");
        std::fs::create_dir_all(folder.join("inner")).unwrap();
        std::fs::write(folder.join("inner/b.txt"), "b").unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap();
        std::fs::write(root.0.join("files/a.txt"), "a").unwrap();
        let outside = Root::new("outside");
        std::fs::write(outside.0.join("files/kept.txt"), "kept").unwrap();
        std::os::unix::fs::symlink(outside.0.join("files"), root.0.join("files/link")).unwrap();
        std::fs::write(root.0.join("info/a.txt.trashinfo"), "[Trash Info]").unwrap();
        std::fs::write(root.0.join("directorysizes"), "1 2 folder").unwrap();
        empty(&root.0).unwrap();
        let leftovers: Vec<_> = ["files", "info"]
            .into_iter()
            .flat_map(|name| std::fs::read_dir(root.0.join(name)).unwrap())
            .collect();
        assert!(leftovers.is_empty());
        assert!(!root.0.join("directorysizes").exists());
        assert!(outside.0.join("files/kept.txt").exists());
    }

    #[test]
    fn emptying_a_missing_trash_succeeds() {
        let root = Root::new("missing");
        empty(&root.0.join("nowhere")).unwrap();
    }
}
