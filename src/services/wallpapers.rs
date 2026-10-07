use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use crate::services::thumbnails::{self, Size};

pub const EXTENSIONS: [&str; 7] = ["jpg", "jpeg", "png", "webp", "avif", "bmp", "svg"];
pub const THUMBNAILED: [&str; 7] = ["jpg", "jpeg", "png", "webp", "tif", "tiff", "svg"];

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

impl Entry {
    pub fn thumbnailed(&self) -> bool {
        !self.is_dir
            && THUMBNAILED
                .iter()
                .any(|extension| self.name.ends_with(&format!(".{extension}")))
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Progress {
    Idle,
    Starting,
    Part(f64),
}

type Listener = Box<dyn Fn()>;
type FileListener = Box<dyn Fn(&Path)>;

pub struct Wallpapers {
    directory: RefCell<PathBuf>,
    history: RefCell<Vec<PathBuf>>,
    position: Cell<usize>,
    query: RefCell<String>,
    progress: Cell<Progress>,
    generation: Cell<u64>,
    changed: RefCell<Vec<Listener>>,
    applied: RefCell<Vec<Listener>>,
    progressed: RefCell<Vec<Listener>>,
    thumbnailed: RefCell<Vec<FileListener>>,
}

impl Wallpapers {
    pub fn new() -> Rc<Self> {
        let folder = start_folder(
            &pictures().join("Wallpapers"),
            Path::new(&crate::core::config::current().wallpaper),
        );
        Rc::new(Wallpapers {
            directory: RefCell::new(folder.clone()),
            history: RefCell::new(vec![folder]),
            position: Cell::new(0),
            query: RefCell::new(String::new()),
            progress: Cell::new(Progress::Idle),
            generation: Cell::new(0),
            changed: RefCell::new(Vec::new()),
            applied: RefCell::new(Vec::new()),
            progressed: RefCell::new(Vec::new()),
            thumbnailed: RefCell::new(Vec::new()),
        })
    }

    pub fn directory(&self) -> PathBuf {
        self.directory.borrow().clone()
    }

    pub fn progress(&self) -> Progress {
        self.progress.get()
    }

    pub fn on_changed(&self, listener: impl Fn() + 'static) {
        self.changed.borrow_mut().push(Box::new(listener));
    }

    pub fn on_applied(&self, listener: impl Fn() + 'static) {
        self.applied.borrow_mut().push(Box::new(listener));
    }

    pub fn on_progress(&self, listener: impl Fn() + 'static) {
        self.progressed.borrow_mut().push(Box::new(listener));
    }

    pub fn on_thumbnail(&self, listener: impl Fn(&Path) + 'static) {
        self.thumbnailed.borrow_mut().push(Box::new(listener));
    }

    pub fn forget_listeners(&self) {
        self.changed.borrow_mut().clear();
        self.progressed.borrow_mut().clear();
        self.thumbnailed.borrow_mut().clear();
    }

    pub fn stop_thumbnails(&self) {
        self.generation.set(self.generation.get() + 1);
        self.set_progress(Progress::Idle);
    }

    pub fn entries(&self) -> Vec<Entry> {
        let query = self.query.borrow().to_lowercase();
        let terms: Vec<&str> = query.split(' ').filter(|term| !term.is_empty()).collect();
        let Ok(listing) = std::fs::read_dir(&*self.directory.borrow()) else {
            return Vec::new();
        };
        let mut entries: Vec<(SystemTime, Entry)> = listing
            .flatten()
            .filter_map(|item| {
                let name = item.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    return None;
                }
                let path = item.path();
                let metadata = std::fs::metadata(&path).ok()?;
                let is_dir = metadata.is_dir();
                if !is_dir && !matches(&name, &terms) {
                    return None;
                }
                if std::fs::File::open(&path).is_err() && !is_dir {
                    return None;
                }
                let time = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                Some((time, Entry { path, name, is_dir }))
            })
            .collect();
        entries.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.name.cmp(&right.1.name)));
        entries.into_iter().map(|(_, entry)| entry).collect()
    }

    pub fn set_query(&self, query: &str) {
        if *self.query.borrow() == query {
            return;
        }
        self.query.replace(query.to_owned());
        self.announce(&self.changed);
    }

    pub fn set_directory(&self, path: &str) {
        let trimmed = path.trim_end_matches('/');
        let path = PathBuf::from(if trimmed.is_empty() { "/" } else { trimmed });
        let folder = if path.is_dir() {
            path
        } else if path.is_file() {
            match path.parent() {
                Some(parent) => parent.to_path_buf(),
                None => return,
            }
        } else {
            return;
        };
        self.go(folder, true);
    }

    pub fn navigate_up(&self) {
        let parent = self.directory().parent().map(Path::to_path_buf);
        if let Some(parent) = parent {
            self.go(parent, true);
        }
    }

    pub fn navigate_back(&self) {
        let position = self.position.get();
        if position == 0 {
            return;
        }
        self.position.set(position - 1);
        let folder = self.history.borrow()[position - 1].clone();
        self.go(folder, false);
    }

    pub fn navigate_forward(&self) {
        let position = self.position.get();
        if position + 1 >= self.history.borrow().len() {
            return;
        }
        self.position.set(position + 1);
        let folder = self.history.borrow()[position + 1].clone();
        self.go(folder, false);
    }

    fn go(&self, folder: PathBuf, remember: bool) {
        if remember {
            let position = self.position.get();
            let mut history = self.history.borrow_mut();
            if history.get(position) != Some(&folder) {
                history.truncate(position + 1);
                history.push(folder.clone());
                self.position.set(history.len() - 1);
            }
        }
        if *self.directory.borrow() == folder {
            return;
        }
        self.directory.replace(folder);
        self.announce(&self.changed);
    }

    pub fn select(&self, path: &Path, dark: bool) {
        if path.is_dir() {
            self.set_directory(&path.to_string_lossy());
            return;
        }
        self.apply(path, dark);
    }

    pub fn apply(&self, path: &Path, dark: bool) {
        let path = path.to_string_lossy();
        if path.is_empty() {
            return;
        }
        crate::theming::switchwall::detach(&["--mode", mode(dark), "--image", &path]);
        self.announce(&self.applied);
    }

    pub fn random(&self, dark: bool) {
        let entries = self.entries();
        if entries.is_empty() {
            return;
        }
        let index = glib::random_int_range(0, entries.len() as i32) as usize;
        self.select(&entries[index].path, dark);
    }

    pub fn open_fallback_picker(&self, dark: bool) {
        crate::theming::switchwall::detach(&["--mode", mode(dark)]);
    }

    pub fn generate_thumbnails(self: &Rc<Self>, size: Size) {
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        let files: Vec<PathBuf> = std::fs::read_dir(self.directory())
            .map(|listing| {
                listing
                    .flatten()
                    .map(|item| item.path())
                    .filter(|path| path.is_file())
                    .collect()
            })
            .unwrap_or_default();
        self.set_progress(Progress::Starting);
        let wallpapers = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let total = files.len();
            for (done, file) in files.into_iter().enumerate() {
                let made = {
                    let file = file.clone();
                    gtk4::gio::spawn_blocking(move || thumbnails::generate(&file, size))
                        .await
                        .unwrap_or(false)
                };
                let Some(wallpapers) = wallpapers.upgrade() else {
                    return;
                };
                if wallpapers.generation.get() != generation {
                    return;
                }
                wallpapers.set_progress(Progress::Part((done + 1) as f64 / total as f64));
                if made {
                    for listener in wallpapers.thumbnailed.borrow().iter() {
                        listener(&file);
                    }
                }
            }
            if let Some(wallpapers) = wallpapers.upgrade()
                && wallpapers.generation.get() == generation
            {
                wallpapers.set_progress(Progress::Idle);
            }
        });
    }

    fn set_progress(&self, progress: Progress) {
        if self.progress.replace(progress) != progress {
            self.announce(&self.progressed);
        }
    }

    fn announce(&self, listeners: &RefCell<Vec<Listener>>) {
        for listener in listeners.borrow().iter() {
            listener();
        }
    }
}

fn start_folder(wallpapers: &Path, current: &Path) -> PathBuf {
    if wallpapers.is_dir() {
        return wallpapers.to_path_buf();
    }
    current
        .parent()
        .filter(|folder| folder.is_dir())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| wallpapers.to_path_buf())
}

pub fn pictures() -> PathBuf {
    glib::user_special_dir(glib::UserDirectory::Pictures).unwrap_or_else(glib::home_dir)
}

fn mode(dark: bool) -> &'static str {
    if dark { "dark" } else { "light" }
}

fn matches(name: &str, terms: &[&str]) -> bool {
    let lower = name.to_lowercase();
    let Some((stem, extension)) = lower.rsplit_once('.') else {
        return false;
    };
    if !EXTENSIONS.contains(&extension) {
        return false;
    }
    let mut rest = stem;
    for term in terms {
        let Some(at) = rest.find(term) else {
            return false;
        };
        rest = &rest[at + term.len()..];
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_wallpapers_folder_the_selector_starts_where_the_wallpaper_is() {
        let root = std::env::temp_dir().join(format!("proscenio-start-{}", std::process::id()));
        let wallpapers = root.join("Pictures/Wallpapers");
        let shipped = root.join("share/proscenio");
        std::fs::create_dir_all(&shipped).unwrap();
        let current = shipped.join("default_wallpaper.png");
        std::fs::write(&current, b"").unwrap();

        assert_eq!(start_folder(&wallpapers, &current), shipped);
        assert_eq!(start_folder(&wallpapers, Path::new("")), wallpapers);
        std::fs::create_dir_all(&wallpapers).unwrap();
        assert_eq!(start_folder(&wallpapers, &current), wallpapers);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_filter_wants_every_word_in_order_and_a_wallpaper_extension() {
        let cases = [
            ("Sunset Beach.JPG", "", true),
            ("notes.txt", "", false),
            ("sunset beach.png", "sun", true),
            ("sunset beach.png", "sun beach", true),
            ("sunset beach.png", "beach sun", false),
            ("sunset beach.png", "png", false),
            ("mountain.avif", "moun", true),
        ];
        for (name, query, expected) in cases {
            let lower = query.to_lowercase();
            let terms: Vec<&str> = lower.split(' ').filter(|term| !term.is_empty()).collect();
            assert_eq!(matches(name, &terms), expected, "{name} / {query}");
        }
    }
}
