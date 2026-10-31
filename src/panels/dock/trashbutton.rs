use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use super::menuitems::{MenuItem, build_trash_items};
use super::trashdialog::confirm_empty;
use super::{Dock, GAPS_OUT};
use crate::platform::system::trash::{self, Watcher};
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;

impl Dock {
    pub(super) fn make_trash_button(
        self: &Rc<Self>,
        app: &gtk4::Application,
    ) -> (RippleButton, Watcher) {
        let item_count = Rc::new(Cell::new(0usize));
        let image = self.paint_icon({
            let item_count = item_count.clone();
            move |_| trash::choose_icon(item_count.get()).to_owned()
        });
        let button = RippleButton::new(&self.theme);
        button.set_radius(rounding::NORMAL as f64);
        let background = self.metrics.background();
        button.set_background_size(background as f32, background as f32);
        button.set_size_request(background, self.metrics.button_height());
        button.set_margin_top(GAPS_OUT);
        button.set_valign(gtk4::Align::Fill);
        button.set_content(&Centered::integral(&image), 0, 0);

        let watcher = trash::watch({
            let item_count = item_count.clone();
            let image = image.downgrade();
            move |count| {
                item_count.set(count);
                if let Some(image) = image.upgrade() {
                    image.queue_draw();
                }
            }
        });

        button.connect_clicked(|_| trash::open());
        let is_asking: Rc<Cell<bool>> = Rc::default();
        button.connect_alt({
            let dock = Rc::downgrade(self);
            let button = button.downgrade();
            let app = app.clone();
            move |_| {
                let (Some(dock), Some(button)) = (dock.upgrade(), button.upgrade()) else {
                    return;
                };
                let theme = dock.theme.clone();
                let (app, is_asking) = (app.clone(), is_asking.clone());
                let items = build_trash_items(item_count.get());
                dock.menu.show(&button, items, move |item| match item {
                    MenuItem::OpenTrash => trash::open(),
                    MenuItem::EmptyTrash { .. } => ask_to_empty(&app, &theme, &is_asking),
                    _ => {}
                });
            }
        });

        let file_drop = gtk4::DropTarget::new(
            gdk::FileList::static_type(),
            gdk::DragAction::COPY | gdk::DragAction::MOVE,
        );
        file_drop.connect_drop(|_, value, _, _| {
            let Ok(list) = value.get::<gdk::FileList>() else {
                return false;
            };
            let paths: Vec<PathBuf> = list.files().iter().filter_map(|file| file.path()).collect();
            if paths.is_empty() {
                return false;
            }
            gio::spawn_blocking(move || {
                for path in paths {
                    let result = gio::File::for_path(&path).trash(gio::Cancellable::NONE);
                    if let Err(error) = result {
                        eprintln!("ERROR: {}: {error}", path.display());
                    }
                }
            });
            true
        });
        button.add_controller(file_drop);
        (button, watcher)
    }
}

fn ask_to_empty(app: &gtk4::Application, theme: &SharedTheme, is_asking: &Rc<Cell<bool>>) {
    if is_asking.replace(true) {
        return;
    }
    confirm_empty(
        app,
        theme,
        || {
            gio::spawn_blocking(|| {
                if let Err(error) = trash::empty(&trash::directory()) {
                    eprintln!("ERROR: {error}");
                }
            });
        },
        {
            let is_asking = is_asking.clone();
            move || is_asking.set(false)
        },
    );
}
