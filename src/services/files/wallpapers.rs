use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use crate::services::files::thumbnails::{self, Size};

pub const EXTENSIONS: [&str; 7] = ["jpg", "jpeg", "png", "webp", "avif", "bmp", "svg"];
pub const THUMBNAIL_EXTENSIONS: [&str; 6] = ["jpg", "jpeg", "png", "webp", "bmp", "svg"];

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

impl Entry {
    pub fn supports_thumbnail(&self) -> bool {
        !self.is_dir && has_thumbnail_extension(&self.name)
    }
}

fn has_thumbnail_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        THUMBNAIL_EXTENSIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(extension))
    })
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
    thumbnail_queue: RefCell<VecDeque<PathBuf>>,
    thumbnail_count: Cell<usize>,
    change_listeners: RefCell<Vec<Listener>>,
    apply_listeners: RefCell<Vec<Listener>>,
    progress_listeners: RefCell<Vec<Listener>>,
    thumbnail_listeners: RefCell<Vec<FileListener>>,
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
            thumbnail_queue: RefCell::default(),
            thumbnail_count: Cell::new(0),
            change_listeners: RefCell::new(Vec::new()),
            apply_listeners: RefCell::new(Vec::new()),
            progress_listeners: RefCell::new(Vec::new()),
            thumbnail_listeners: RefCell::new(Vec::new()),
        })
    }

    pub fn directory(&self) -> PathBuf {
        self.directory.borrow().clone()
    }

    pub fn progress(&self) -> Progress {
        self.progress.get()
    }

    pub fn on_changed(&self, listener: impl Fn() + 'static) {
        self.change_listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn on_applied(&self, listener: impl Fn() + 'static) {
        self.apply_listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn on_progress(&self, listener: impl Fn() + 'static) {
        self.progress_listeners
            .borrow_mut()
            .push(Box::new(listener));
    }

    pub fn on_thumbnail(&self, listener: impl Fn(&Path) + 'static) {
        self.thumbnail_listeners
            .borrow_mut()
            .push(Box::new(listener));
    }

    pub fn forget_listeners(&self) {
        self.change_listeners.borrow_mut().clear();
        self.progress_listeners.borrow_mut().clear();
        self.thumbnail_listeners.borrow_mut().clear();
    }

    pub fn stop_thumbnails(&self) {
        self.generation.set(self.generation.get() + 1);
        self.thumbnail_queue.borrow_mut().clear();
        self.set_progress(Progress::Idle);
    }

    pub async fn entries(&self) -> Vec<Entry> {
        let directory = self.directory();
        let query = self.query.borrow().to_lowercase();
        gtk4::gio::spawn_blocking(move || {
            let terms: Vec<&str> = query.split(' ').filter(|term| !term.is_empty()).collect();
            list_entries(&directory, &terms)
        })
        .await
        .unwrap_or_default()
    }

    pub fn set_query(&self, query: &str) {
        if *self.query.borrow() == query {
            return;
        }
        self.query.replace(query.to_owned());
        self.announce(&self.change_listeners);
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

    fn go(&self, folder: PathBuf, should_remember: bool) {
        if should_remember {
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
        self.announce(&self.change_listeners);
    }

    pub fn select(&self, path: &Path, is_dark: bool) {
        if path.is_dir() {
            self.set_directory(&path.to_string_lossy());
            return;
        }
        self.apply(path, is_dark);
    }

    pub fn apply(&self, path: &Path, is_dark: bool) {
        let path = path.to_string_lossy();
        if path.is_empty() {
            return;
        }
        crate::theming::switchwall::detach(&["--mode", mode(is_dark), "--image", &path]);
        self.announce(&self.apply_listeners);
    }

    pub fn random(self: &Rc<Self>, is_dark: bool) {
        let wallpapers = self.clone();
        glib::spawn_future_local(async move {
            let pictures = keep_pictures(wallpapers.entries().await);
            if pictures.is_empty() {
                return;
            }
            let index = glib::random_int_range(0, pictures.len() as i32) as usize;
            wallpapers.select(&pictures[index].path, is_dark);
        });
    }

    pub fn open_fallback_picker(&self, is_dark: bool) {
        crate::theming::switchwall::detach(&["--mode", mode(is_dark)]);
    }

    pub fn generate_thumbnails(self: &Rc<Self>, size: Size) {
        let files = list_thumbnail_files(&self.directory());
        self.thumbnail_count.set(files.len());
        self.thumbnail_queue.replace(files.into());
        self.start_thumbnails(size);
    }

    pub fn generate_thumbnail(self: &Rc<Self>, file: &Path, size: Size) {
        if !is_thumbnail_file(file)
            || self
                .thumbnail_queue
                .borrow()
                .iter()
                .any(|queued| queued == file)
        {
            return;
        }
        let is_generating = self.progress.get() != Progress::Idle;
        if !is_generating {
            self.thumbnail_count.set(0);
        }
        self.thumbnail_queue
            .borrow_mut()
            .push_back(file.to_path_buf());
        self.thumbnail_count.set(self.thumbnail_count.get() + 1);
        if !is_generating {
            self.start_thumbnails(size);
        }
    }

    fn start_thumbnails(self: &Rc<Self>, size: Size) {
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        self.set_progress(Progress::Starting);
        let wallpapers = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Some(file) = wallpapers
                .upgrade()
                .filter(|wallpapers| wallpapers.generation.get() == generation)
                .and_then(|wallpapers| wallpapers.thumbnail_queue.borrow_mut().pop_front())
            {
                let was_generated = {
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
                let total = wallpapers.thumbnail_count.get();
                let done = total - wallpapers.thumbnail_queue.borrow().len();
                wallpapers.set_progress(Progress::Part(done as f64 / total as f64));
                if was_generated {
                    for listener in wallpapers.thumbnail_listeners.borrow().iter() {
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
            self.announce(&self.progress_listeners);
        }
    }

    fn announce(&self, listeners: &RefCell<Vec<Listener>>) {
        for listener in listeners.borrow().iter() {
            listener();
        }
    }
}

fn list_entries(directory: &Path, terms: &[&str]) -> Vec<Entry> {
    let Ok(listing) = std::fs::read_dir(directory) else {
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
            let is_listed = is_dir
                || (metadata.is_file()
                    && matches(&name, terms)
                    && std::fs::File::open(&path).is_ok());
            if !is_listed {
                return None;
            }
            let time = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((time, Entry { path, name, is_dir }))
        })
        .collect();
    entries.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.name.cmp(&right.1.name)));
    entries.into_iter().map(|(_, entry)| entry).collect()
}

fn keep_pictures(entries: Vec<Entry>) -> Vec<Entry> {
    entries.into_iter().filter(|entry| !entry.is_dir).collect()
}

fn list_thumbnail_files(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .map(|listing| {
            listing
                .flatten()
                .map(|item| item.path())
                .filter(|path| is_thumbnail_file(path))
                .collect()
        })
        .unwrap_or_default()
}

fn is_thumbnail_file(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy())
        .is_some_and(|name| !name.starts_with('.') && has_thumbnail_extension(&name))
        && path.is_file()
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

fn mode(is_dark: bool) -> &'static str {
    if is_dark { "dark" } else { "light" }
}

fn matches(name: &str, terms: &[&str]) -> bool {
    let lowercase_name = name.to_lowercase();
    let Some((stem, extension)) = lowercase_name.rsplit_once('.') else {
        return false;
    };
    if !EXTENSIONS.contains(&extension) {
        return false;
    }
    let mut rest = stem;
    for term in terms {
        let Some(index) = rest.find(term) else {
            return false;
        };
        rest = &rest[index + term.len()..];
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
        let share_folder = root.join("share/proscenio");
        std::fs::create_dir_all(&share_folder).unwrap();
        let current = share_folder.join("default_wallpaper.png");
        std::fs::write(&current, b"").unwrap();

        assert_eq!(start_folder(&wallpapers, &current), share_folder);
        assert_eq!(start_folder(&wallpapers, Path::new("")), wallpapers);
        std::fs::create_dir_all(&wallpapers).unwrap();
        assert_eq!(start_folder(&wallpapers, &current), wallpapers);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn thumbnails_are_made_only_for_the_listed_pictures() {
        let root =
            std::env::temp_dir().join(format!("proscenio-thumbnails-{}", std::process::id()));
        std::fs::create_dir_all(root.join("folder.png")).unwrap();
        for name in [
            "beach.png",
            "notes.txt",
            ".hidden.png",
            "mountain.avif",
            "archive",
        ] {
            std::fs::write(root.join(name), b"").unwrap();
        }

        assert_eq!(list_thumbnail_files(&root), [root.join("beach.png")]);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_picture_name_on_something_other_than_a_file_is_not_listed() {
        let root = std::env::temp_dir().join(format!("proscenio-special-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("beach.png"), b"").unwrap();
        std::os::unix::fs::symlink("/dev/null", root.join("device.png")).unwrap();

        let names: Vec<String> = list_entries(&root, &[])
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(names, ["beach.png"]);
    }

    #[test]
    fn a_random_pick_never_lands_on_a_folder() {
        let root = std::env::temp_dir().join(format!("proscenio-random-{}", std::process::id()));
        std::fs::create_dir_all(root.join("album")).unwrap();
        std::fs::write(root.join("beach.png"), b"").unwrap();

        let names: Vec<String> = keep_pictures(list_entries(&root, &[]))
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(names, ["beach.png"]);
    }

    #[test]
    fn bmp_and_upper_case_pictures_get_a_thumbnail() {
        let cases = [
            ("photo.bmp", false, true),
            ("SCAN.BMP", false, true),
            ("Sunset Beach.JPG", false, true),
            ("mountain.avif", false, false),
            ("photo.bmp", true, false),
        ];
        for (name, is_dir, expected) in cases {
            let entry = Entry {
                path: PathBuf::from(name),
                name: name.to_owned(),
                is_dir,
            };
            assert_eq!(entry.supports_thumbnail(), expected, "{name}");
        }
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
            let lowercase_query = query.to_lowercase();
            let terms: Vec<&str> = lowercase_query
                .split(' ')
                .filter(|term| !term.is_empty())
                .collect();
            assert_eq!(matches(name, &terms), expected, "{name} / {query}");
        }
    }
}
