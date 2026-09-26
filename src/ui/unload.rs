use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const SETTLE: Duration = Duration::from_millis(1500);
const AFTER_TOOLTIP_POPUP: Duration = Duration::from_millis(1000);
const GROWTH_BEFORE_TRIM: usize = 1024 * 1024;
const PAGE: usize = 4096;

unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

thread_local! {
    static PENDING: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

fn resident() -> Result<usize, String> {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| statm.split_whitespace().nth(1)?.parse::<usize>().ok())
        .map(|pages| pages * PAGE)
        .ok_or_else(|| "/proc/self/statm cannot be read".to_owned())
}

pub fn trimming() -> impl Fn() -> Result<(), String> {
    let trimmed_at = Cell::new(resident().unwrap_or(0));
    move || {
        let now = resident()?;
        if now > trimmed_at.get() + GROWTH_BEFORE_TRIM {
            unsafe {
                malloc_trim(0);
            }
            trimmed_at.set(resident()?);
        } else if now < trimmed_at.get() {
            trimmed_at.set(now);
        }
        Ok(())
    }
}

pub fn when_hidden(widget: &impl IsA<gtk4::Widget>) {
    let hides = Rc::new(Cell::new(0u64));
    widget.connect_hide(move |widget| {
        let widget = widget.upcast_ref::<gtk4::Widget>().downgrade();
        let hide = hides.get() + 1;
        hides.set(hide);
        let hides = hides.clone();
        glib::timeout_add_local_once(AFTER_TOOLTIP_POPUP, move || {
            let Some(widget) = widget.upgrade() else {
                return;
            };
            if widget.is_visible() || hides.get() != hide {
                return;
            }
            widget.unrealize();
            trim();
        });
    });
}

pub fn discard(window: &gtk4::Window) {
    let mut widgets = Vec::new();
    descendants(window.upcast_ref(), &mut widgets);
    window.destroy();
    unsafe {
        window.run_dispose();
    }
    loop {
        let detached: Vec<gtk4::Widget> = widgets
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .filter(|widget| widget.parent().is_none() && !widget.is::<gtk4::Native>())
            .collect();
        if detached.is_empty() {
            return;
        }
        for widget in &detached {
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

fn descendants(parent: &gtk4::Widget, into: &mut Vec<glib::WeakRef<gtk4::Widget>>) {
    let mut child = parent.first_child();
    while let Some(widget) = child {
        into.push(widget.downgrade());
        descendants(&widget, into);
        child = widget.next_sibling();
    }
}

pub fn trim() {
    unsafe {
        malloc_trim(0);
    }
    PENDING.with(|pending| {
        if let Some(source) = pending.borrow_mut().take() {
            source.remove();
        }
        let source = glib::timeout_add_local_once(SETTLE, || {
            PENDING.with(|pending| pending.borrow_mut().take());
            unsafe {
                malloc_trim(0);
            }
        });
        pending.replace(Some(source));
    });
}
