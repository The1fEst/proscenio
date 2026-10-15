use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::launcher::{IconType, Item, Run};
use super::{Overview, SEARCH_WIDTH};
use crate::core::i18n::tr;
use crate::services::files::cliphist;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

const ITEM_MARGIN: i32 = 10;
const ITEM_PADDING: i32 = 10;
const ITEM_VERTICAL_PADDING: i32 = 6;
const APP_ICON: i32 = 35;
const ACTION_BUTTON: i32 = 34;
const ACTION_ICON: i32 = 20;
const PREVIEW_HEIGHT: f64 = 140.0;
const EMOJI_QT_SCALE: f64 = 20.0 / 23.0;
const IMAGE_TOP_LINE: i32 = 24;
const IMAGE_PREVIEW_TOP: i32 = 40;
const IMAGE_SQUEEZE: i32 = 6;
const BLUR_ICON: f64 = 28.0;

pub(super) struct Row {
    pub(super) item: Item,
    pub(super) widget: gtk4::Overlay,
    pub(super) button: RippleButton,
    kind: gtk4::Label,
    name: gtk4::Label,
    glyph: Option<gtk4::Label>,
    verb: gtk4::Label,
    symbols: Vec<gtk4::Label>,
    plain_markup: String,
    highlighted_markup: String,
    pub(super) is_hovered: Cell<bool>,
}

impl Row {
    pub(super) fn paint(&self, is_highlighted: bool) {
        let foreground = if is_highlighted {
            "colOnPrimaryContainer"
        } else {
            "m3onSurface"
        };
        self.button.set_toggled(is_highlighted);
        text::set_color(
            &self.kind,
            if is_highlighted {
                "colOnPrimaryContainer"
            } else {
                "colSubtext"
            },
        );
        text::set_color(&self.name, foreground);
        self.name.set_markup(if is_highlighted {
            &self.plain_markup
        } else {
            &self.highlighted_markup
        });
        if let Some(glyph) = &self.glyph {
            text::set_color(glyph, foreground);
        }
        for symbol in &self.symbols {
            text::set_color(symbol, foreground);
        }
        self.verb.set_visible(is_highlighted);
    }
}

impl Overview {
    pub(super) fn row(self: &Rc<Self>, item: Item, query: &str) -> Rc<Row> {
        let raw_entry = item.raw_entry.clone();
        let is_image = !raw_entry.is_empty() && cliphist::is_image(&raw_entry);
        let is_monospace = item.is_monospace;

        let button = RippleButton::new(&self.theme);
        button.set_look(Look {
            background: |theme| {
                crate::ui::theme::transparentize(theme.colors.col_primary_container, 1.0)
            },
            hover: |theme| theme.colors.col_primary_container,
            toggled: |theme| theme.colors.col_primary_container,
            toggled_hover: |theme| theme.colors.col_primary_container,
            ripple: |theme| theme.colors.col_primary_container_active,
            ripple_toggled: |theme| theme.colors.col_primary_container_active,
        });
        button.set_radius(crate::ui::theme::rounding::NORMAL as f64);
        button.set_background_insets(ITEM_MARGIN as f32, 0.0);
        button.set_cursor_from_name(Some("pointer"));
        button.set_can_focus(false);

        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);

        let mut glyph = None;
        match item.icon_type {
            IconType::System => {
                line.append(&app_icon(&item.icon, APP_ICON, &button));
            }
            IconType::Material => {
                let symbol = text::symbol(&item.icon, 30.0);
                line.append(&Centered::new(&symbol));
                glyph = Some(symbol);
            }
            IconType::Text => {
                let label = text::styled_sized(&item.icon, pixel_size::LARGER);
                text::set_font(
                    &label,
                    Family::Main,
                    pixel_size::LARGER as f64 * EMOJI_QT_SCALE,
                    "wght=450",
                );
                label.set_valign(gtk4::Align::Center);
                line.append(&label);
                glyph = Some(label);
            }
            IconType::None => line.set_spacing(0),
        }

        let kind = text::styled_sized(&item.kind, pixel_size::SMALLER);
        kind.set_xalign(0.0);
        let kind_row = gtk4::Overlay::new();
        let kind_holder = Centered::filling_width(&kind);
        kind_holder.set_hexpand(true);
        kind_holder.set_visible(!item.kind.is_empty() && item.kind != tr("App"));
        kind_row.set_child(Some(&kind_holder));

        let mut symbols = Vec::new();
        let mut action_places: Vec<(gtk4::Box, gtk4::Box)> = Vec::new();
        if is_image {
            let actions = self.actions(&item, &mut symbols);
            let place = placeholder(&actions);
            place.set_margin_end(ITEM_VERTICAL_PADDING);
            place.set_halign(gtk4::Align::End);
            place.set_valign(gtk4::Align::Start);
            kind_holder.set_size_request(-1, IMAGE_TOP_LINE);
            kind_row.add_overlay(&place);
            action_places.push((actions, place));
        }

        let name = gtk4::Label::new(None);
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_ellipsize(pango::EllipsizeMode::End);
        text::set_font(
            &name,
            if is_monospace {
                Family::Monospace
            } else {
                Family::Main
            },
            pixel_size::SMALL as f64,
            "wght=450",
        );
        if raw_entry.is_empty() {
            name.set_single_line_mode(true);
        } else {
            name.set_wrap(true);
            name.set_wrap_mode(pango::WrapMode::WordChar);
            name.set_lines(3);
        }
        let plain_markup = glib::markup_escape_text(&item.name).to_string();
        let highlighted_markup = if is_monospace {
            plain_markup.clone()
        } else {
            highlight(&item.name, query, &self.theme)
        };
        name.set_visible(!is_image);

        let name_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        if !raw_entry.is_empty() && *self.clipboard.borrow() == item.name {
            name_row.append(&check_mark());
        }
        if raw_entry.is_empty() {
            let holder = Centered::filling_width(&name);
            holder.set_hexpand(true);
            holder.set_visible(!is_image);
            name_row.append(&holder);
        } else {
            name_row.append(&name);
        }

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        column.set_hexpand(true);
        column.set_valign(gtk4::Align::Center);
        column.append(&kind_row);
        column.append(&name_row);
        let preview = if is_image {
            let preview = self.preview(&raw_entry, item.should_blur);
            preview.set_margin_top(IMAGE_PREVIEW_TOP - IMAGE_TOP_LINE);
            preview.set_margin_bottom(ITEM_VERTICAL_PADDING - IMAGE_SQUEEZE);
            preview
        } else {
            let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            spacer.set_margin_bottom(ITEM_VERTICAL_PADDING);
            spacer.upcast()
        };
        column.append(&preview);
        line.append(&column);

        let verb = text::styled_sized(&item.verb, pixel_size::NORMAL);
        text::set_color(&verb, "colOnPrimaryContainer");
        let verb_holder = Centered::new(&verb);
        verb_holder.set_valign(gtk4::Align::Center);
        line.append(&verb_holder);
        verb.set_visible(false);

        if !is_image && !item.actions.is_empty() {
            let actions = self.actions(&item, &mut symbols);
            let place = placeholder(&actions);
            place.set_valign(gtk4::Align::Center);
            line.append(&place);
            action_places.push((actions, place));
        }

        button.set_content(&line, ITEM_MARGIN + ITEM_PADDING, ITEM_VERTICAL_PADDING);

        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&button));
        for (actions, _) in &action_places {
            widget.add_overlay(actions);
        }
        widget.connect_get_child_position(move |overlay, child| {
            let (_, place) = action_places
                .iter()
                .find(|(actions, _)| actions.upcast_ref::<gtk4::Widget>() == child)?;
            let bounds = place.compute_bounds(overlay)?;
            Some(gdk::Rectangle::new(
                bounds.x().round() as i32,
                bounds.y().round() as i32,
                bounds.width().round() as i32,
                bounds.height().round() as i32,
            ))
        });

        let row = Rc::new(Row {
            item,
            widget: widget.clone(),
            button: button.clone(),
            kind,
            name,
            glyph,
            verb,
            symbols,
            plain_markup,
            highlighted_markup,
            is_hovered: Cell::new(false),
        });

        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_enter({
            let row = Rc::downgrade(&row);
            let overview = Rc::downgrade(self);
            move |_, _, _| {
                let (Some(row), Some(overview)) = (row.upgrade(), overview.upgrade()) else {
                    return;
                };
                row.is_hovered.set(true);
                overview.repaint();
            }
        });
        motion_controller.connect_leave({
            let row = Rc::downgrade(&row);
            let overview = Rc::downgrade(self);
            move |_| {
                let (Some(row), Some(overview)) = (row.upgrade(), overview.upgrade()) else {
                    return;
                };
                row.is_hovered.set(false);
                overview.repaint();
            }
        });
        widget.add_controller(motion_controller);

        button.connect_clicked({
            let overview = Rc::downgrade(self);
            let run = row.item.run.clone();
            move |_| {
                if let Some(overview) = overview.upgrade() {
                    overview.hide();
                    overview.launcher.execute(&run);
                }
            }
        });
        row
    }

    fn actions(self: &Rc<Self>, item: &Item, symbols: &mut Vec<gtk4::Label>) -> gtk4::Box {
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        for action in item.actions.iter().take(4) {
            let button = RippleButton::new(&self.theme);
            button.set_look(Look {
                hover: |theme| theme.colors.col_secondary_container_hover,
                ripple: |theme| theme.colors.col_secondary_container_active,
                ..Look::default()
            });
            button.set_size_request(ACTION_BUTTON, ACTION_BUTTON);
            button.set_can_focus(false);
            if action.icon_type == IconType::Material || action.icon.is_empty() {
                let name = if action.icon.is_empty() {
                    "video_settings"
                } else {
                    action.icon.as_str()
                };
                let symbol = text::symbol(name, pixel_size::HUGEASS as f64);
                symbols.push(symbol.clone());
                button.set_content(&Centered::new(&symbol), 0, 0);
            } else {
                button.set_content(&app_icon(&action.icon, ACTION_ICON, &button), 0, 0);
            }
            let tip = Tooltip::new(&button, &self.theme, tooltip::Kind::Styled);
            tip.set_text(&action.name);
            tooltip::hover_delay(&button, &tip, 0);
            button.connect_clicked({
                let launcher = self.launcher.clone();
                let run: Run = action.run.clone();
                move |_| launcher.execute(&run)
            });
            actions.append(&button);
        }
        actions
    }

    fn preview(&self, entry: &str, should_blur: bool) -> gtk4::Widget {
        let (width, height) = cliphist::image_size(entry).unwrap_or((0, 0));
        let max_width = SEARCH_WIDTH - ((ITEM_MARGIN + ITEM_PADDING) * 2) as f64;
        let scale = if width == 0 || height == 0 {
            0.0
        } else {
            (max_width / width as f64)
                .min(PREVIEW_HEIGHT / height as f64)
                .min(1.0)
        };
        let display_width = (width as f64 * scale).round() as i32;
        let display_height = (height as f64 * scale).round() as i32;

        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Contain);
        picture.set_can_shrink(true);
        picture.set_halign(gtk4::Align::Center);
        picture.set_size_request(display_width, display_height);

        let frame = gtk4::Overlay::new();
        frame.add_css_class("search-preview");
        frame.set_overflow(gtk4::Overflow::Hidden);
        frame.set_hexpand(true);
        frame.set_size_request(-1, display_height);
        frame.set_child(Some(&picture));

        if should_blur {
            picture.add_css_class("search-blurred");
            let veil = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            veil.add_css_class("search-veil");
            veil.set_valign(gtk4::Align::Fill);
            let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            inner.set_valign(gtk4::Align::Center);
            inner.set_vexpand(true);
            let symbol = text::symbol("visibility_off", BLUR_ICON);
            let label = text::styled_sized(&tr("Image hidden"), pixel_size::SMALLIE);
            text::set_color(&label, "colOnSurface");
            inner.append(&Centered::new(&symbol));
            inner.append(&Centered::new(&label));
            veil.append(&inner);
            frame.add_overlay(&veil);
        }

        let number = cliphist::entry_number(entry).unwrap_or(0);
        let path = self.decode_folder.join(number.to_string());
        let scale = picture.scale_factor().max(1);
        let target = if display_width > 0 && display_height > 0 {
            (display_width * scale, display_height * scale)
        } else {
            (-1, -1)
        };
        let entry = entry.to_owned();
        glib::spawn_future_local(async move {
            let decode_result = gio::spawn_blocking({
                let path = path.clone();
                move || ensure_decoded(&path, |file| run_decode(&entry, file))
            })
            .await;
            if decode_result.is_err() {
                return;
            }
            if let Some(texture) = crate::ui::image::texture(path, target).await {
                picture.set_paintable(Some(&texture));
            }
        });
        frame.upcast()
    }
}

fn ensure_decoded(path: &std::path::Path, decode: impl FnOnce(std::fs::File) -> bool) {
    static PARTIAL_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0) {
        return;
    }
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let count = PARTIAL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let partial_path = path.with_extension(format!("part{}-{count}", std::process::id()));
    let Ok(file) = std::fs::File::create(&partial_path) else {
        return;
    };
    let is_complete = decode(file)
        && std::fs::metadata(&partial_path).is_ok_and(|metadata| metadata.len() > 0)
        && std::fs::rename(&partial_path, path).is_ok();
    if !is_complete {
        let _ = std::fs::remove_file(&partial_path);
    }
}

fn run_decode(entry: &str, output: std::fs::File) -> bool {
    let Ok(mut child) = std::process::Command::new("cliphist")
        .arg("decode")
        .stdin(std::process::Stdio::piped())
        .stdout(output)
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = writeln!(stdin, "{entry}");
    }
    child.wait().is_ok_and(|status| status.success())
}

fn app_icon(name: &str, size: i32, anchor: &impl IsA<gtk4::Widget>) -> gtk4::Widget {
    let image = gtk4::Image::new();
    image.set_pixel_size(size);
    image.set_size_request(size, size);
    image.set_valign(gtk4::Align::Center);
    if let Some(display) = gdk::Display::default() {
        let icons = gtk4::IconTheme::for_display(&display);
        let scale = anchor.scale_factor().max(1);
        image.set_paintable(Some(&crate::platform::system::appicon::load_icon(
            &icons,
            name,
            "image-missing",
            size,
            scale,
        )));
    }
    image.upcast()
}

fn placeholder(actions: &gtk4::Box) -> gtk4::Box {
    let (_, width, _, _) = actions.measure(gtk4::Orientation::Horizontal, -1);
    let (_, height, _, _) = actions.measure(gtk4::Orientation::Vertical, -1);
    let place = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    place.set_size_request(width, height);
    place
}

fn check_mark() -> gtk4::Widget {
    let symbol = text::symbol("check", pixel_size::NORMAL as f64);
    text::set_color(&symbol, "m3onPrimary");
    let holder = Centered::new(&symbol);
    let (_, height, _, _) = holder.measure(gtk4::Orientation::Vertical, -1);
    let circle = gtk4::Overlay::new();
    circle.add_css_class("search-check");
    circle.set_size_request(height, height);
    circle.set_valign(gtk4::Align::Center);
    circle.add_overlay(&holder);
    circle.upcast()
}

fn highlight(content: &str, query: &str, theme: &SharedTheme) -> String {
    if query.is_empty() || content == query {
        return glib::markup_escape_text(content).to_string();
    }
    let color = theme.borrow().colors.col_primary;
    let hex = format!(
        "#{:02x}{:02x}{:02x}",
        (color.red() * 255.0).round() as u8,
        (color.green() * 255.0).round() as u8,
        (color.blue() * 255.0).round() as u8
    );
    let query_letters: Vec<char> = query.to_lowercase().chars().collect();
    let mut markup = String::new();
    let mut match_index = 0;
    for letter in content.chars() {
        let letter_markup = glib::markup_escape_text(&letter.to_string()).to_string();
        let lowercase_letter = letter.to_lowercase().next().unwrap_or(letter);
        if match_index < query_letters.len() && lowercase_letter == query_letters[match_index] {
            markup.push_str(&format!(
                "<u><span foreground=\"{hex}\">{letter_markup}</span></u>"
            ));
            match_index += 1;
        } else {
            markup.push_str(&letter_markup);
        }
    }
    markup
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_scratch_folder(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("proscenio-rows-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_decode_that_fails_leaves_nothing_and_the_next_try_succeeds() {
        let folder = make_scratch_folder("failed");
        let path = folder.join("7");
        ensure_decoded(&path, |mut file| {
            file.write_all(b"half").unwrap();
            false
        });
        let leftovers = std::fs::read_dir(&folder).unwrap().count();
        ensure_decoded(&path, |mut file| {
            file.write_all(b"image").unwrap();
            true
        });
        let content = std::fs::read(&path).unwrap();
        let files = std::fs::read_dir(&folder).unwrap().count();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(leftovers, 0);
        assert_eq!(content, b"image");
        assert_eq!(files, 1);
    }

    #[test]
    fn an_empty_cached_file_is_decoded_again() {
        let folder = make_scratch_folder("empty");
        let path = folder.join("8");
        std::fs::write(&path, b"").unwrap();
        ensure_decoded(&path, |mut file| {
            file.write_all(b"image").unwrap();
            true
        });
        let content = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(content, b"image");
    }

    #[test]
    fn a_decode_that_succeeds_with_no_output_leaves_nothing() {
        let folder = make_scratch_folder("silent");
        let path = folder.join("9");
        ensure_decoded(&path, |_| true);
        let files = std::fs::read_dir(&folder).unwrap().count();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(files, 0);
    }

    #[test]
    fn overlapping_decodes_of_one_entry_write_separate_files() {
        let folder = make_scratch_folder("overlap");
        let path = folder.join("11");
        ensure_decoded(&path, |mut outer| {
            outer.write_all(b"half").unwrap();
            ensure_decoded(&path, |mut inner| {
                inner.write_all(b"inner").unwrap();
                true
            });
            outer.write_all(b"-rest").unwrap();
            true
        });
        let content = std::fs::read(&path).unwrap();
        let files = std::fs::read_dir(&folder).unwrap().count();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(content, b"half-rest");
        assert_eq!(files, 1);
    }

    #[test]
    fn a_finished_decode_is_not_run_again() {
        let folder = make_scratch_folder("cached");
        let path = folder.join("10");
        std::fs::write(&path, b"image").unwrap();
        ensure_decoded(&path, |_| panic!("decoded a cached entry"));
        let content = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(content, b"image");
    }
}
