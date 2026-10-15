use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const SETTLE: Duration = Duration::from_millis(1500);
const AFTER_TOOLTIP_POPUP: Duration = Duration::from_millis(1000);
const GROWTH_BEFORE_TRIM_BYTES: usize = 1024 * 1024;
const M_ARENA_MAX: i32 = -8;
const SC_PAGESIZE: i32 = 30;

unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
    fn mallopt(param: i32, value: i32) -> i32;
    fn sysconf(name: i32) -> i64;
}

pub fn single_arena() {
    unsafe {
        mallopt(M_ARENA_MAX, 1);
    }
}

thread_local! {
    static TRIM_TIMER: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

fn read_resident_bytes(page_bytes: usize) -> Result<usize, String> {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| parse_resident_bytes(&statm, page_bytes))
        .ok_or_else(|| "/proc/self/statm cannot be read".to_owned())
}

fn parse_resident_bytes(statm: &str, page_bytes: usize) -> Option<usize> {
    let pages = statm.split_whitespace().nth(1)?.parse::<usize>().ok()?;
    Some(pages * page_bytes)
}

fn read_page_bytes() -> usize {
    unsafe { sysconf(SC_PAGESIZE) as usize }
}

pub fn trimmer() -> impl Fn() -> Result<(), String> {
    let page_bytes = read_page_bytes();
    let baseline_bytes = Cell::new(read_resident_bytes(page_bytes).unwrap_or(0));
    move || {
        let resident_bytes = read_resident_bytes(page_bytes)?;
        if resident_bytes > baseline_bytes.get() + GROWTH_BEFORE_TRIM_BYTES {
            unsafe {
                malloc_trim(0);
            }
            baseline_bytes.set(read_resident_bytes(page_bytes)?);
        } else if resident_bytes < baseline_bytes.get() {
            baseline_bytes.set(resident_bytes);
        }
        Ok(())
    }
}

pub fn unrealize_when_hidden(widget: &impl IsA<gtk4::Widget>) {
    let hide_count = Rc::new(Cell::new(0u64));
    widget.connect_hide(move |widget| {
        let widget = widget.upcast_ref::<gtk4::Widget>().downgrade();
        let hide_number = hide_count.get() + 1;
        hide_count.set(hide_number);
        let hide_count = hide_count.clone();
        glib::timeout_add_local_once(AFTER_TOOLTIP_POPUP, move || {
            let Some(widget) = widget.upgrade() else {
                return;
            };
            if widget.is_visible() || hide_count.get() != hide_number {
                return;
            }
            widget.unrealize();
            trim();
        });
    });
}

pub fn discard(window: &gtk4::Window) {
    let mut widgets = Vec::new();
    collect_descendants(window.upcast_ref(), &mut widgets);
    window.destroy();
    unsafe {
        window.run_dispose();
    }
    loop {
        let orphans: Vec<gtk4::Widget> = widgets
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .filter(|widget| widget.parent().is_none() && !widget.is::<gtk4::Native>())
            .collect();
        if orphans.is_empty() {
            return;
        }
        for widget in &orphans {
            unsafe {
                widget.run_dispose();
            }
        }
        widgets.retain(|weak| {
            weak.upgrade()
                .is_some_and(|widget| widget.parent().is_some())
        });
    }
}

fn collect_descendants(parent: &gtk4::Widget, widgets: &mut Vec<glib::WeakRef<gtk4::Widget>>) {
    let mut child = parent.first_child();
    while let Some(widget) = child {
        widgets.push(widget.downgrade());
        collect_descendants(&widget, widgets);
        child = widget.next_sibling();
    }
}

pub fn trim() {
    unsafe {
        malloc_trim(0);
    }
    TRIM_TIMER.with(|trim_timer| {
        if let Some(source) = trim_timer.borrow_mut().take() {
            source.remove();
        }
        let source = glib::timeout_add_local_once(SETTLE, || {
            TRIM_TIMER.with(|trim_timer| trim_timer.borrow_mut().take());
            unsafe {
                malloc_trim(0);
            }
        });
        trim_timer.replace(Some(source));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resident_pages_count_in_the_page_size() {
        let statm = "5000 250 120 30 0 900 0\n";
        assert_eq!(parse_resident_bytes(statm, 4096), Some(250 * 4096));
        assert_eq!(parse_resident_bytes(statm, 16384), Some(250 * 16384));
        assert_eq!(parse_resident_bytes(statm, 65536), Some(250 * 65536));
        assert_eq!(parse_resident_bytes("5000\n", 4096), None);
    }

    #[test]
    fn read_page_bytes_matches_the_kernel_page_size() {
        const AT_PAGESZ: usize = 6;
        const WORD_BYTES: usize = size_of::<usize>();
        let auxv = std::fs::read("/proc/self/auxv").unwrap();
        let (entries, _) = auxv.as_chunks::<{ WORD_BYTES * 2 }>();
        let kernel_page_bytes = entries.iter().find_map(|entry| {
            let (key, value) = entry.split_at(WORD_BYTES);
            let key = usize::from_ne_bytes(key.try_into().unwrap());
            let value = usize::from_ne_bytes(value.try_into().unwrap());
            (key == AT_PAGESZ).then_some(value)
        });
        assert_eq!(kernel_page_bytes, Some(read_page_bytes()));
    }
}
