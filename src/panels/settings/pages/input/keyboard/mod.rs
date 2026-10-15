pub mod keyoptions;
pub mod shortcuts;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::panels::settings::hyprrows;
use crate::platform::hyprland::hyprconfig::Area;
use crate::platform::hyprland::keybinds::Shortcut;
use crate::platform::system::xkbregistry::{self, Catalog, OptionGroup};
use crate::services::hyprland::hyproptions::HyprOptions;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

pub const OPTIONS: [&str; 5] = [
    "input:kb_layout",
    "input:kb_variant",
    "input:kb_options",
    "input:numlock_by_default",
    "input:resolve_binds_by_sym",
];
const SOURCE_HEIGHT: i32 = 44;
const SOURCE_START: i32 = 12;
const SOURCE_END: i32 = 6;
const SOURCE_SPACING: i32 = 8;
const ROUND_BUTTON: i32 = 32;
const ROUND_ICON: f64 = 20.0;
const ADD_TOP: i32 = 4;
const CHOOSER_HEIGHT: i32 = 260;
const CHOOSER_MARGIN: i32 = 4;
const CANDIDATE_HEIGHT: i32 = 34;
const CANDIDATE_GAP: i32 = 2;
const CANDIDATE_SIDE: i32 = 8;
const CANDIDATE_PADDING: i32 = 8;
const RESULTS_SPACING: i32 = 4;

#[derive(Clone, Debug, PartialEq)]
struct Source {
    code: String,
    variant: String,
}

#[derive(Clone)]
struct Candidate {
    code: String,
    variant: String,
    name: String,
}

fn split(value: &str) -> Vec<String> {
    if value.is_empty() {
        return Vec::new();
    }
    value
        .split(',')
        .map(|part| part.trim().to_owned())
        .collect()
}

fn name_of(catalog: &Catalog, code: &str, variant: &str) -> String {
    let Some(layout) = catalog.layouts.iter().find(|layout| layout.code == code) else {
        return code.to_owned();
    };
    if variant.is_empty() {
        return layout.name.clone();
    }
    layout
        .variants
        .iter()
        .find(|entry| entry.code == variant)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("{} ({variant})", layout.name))
}

fn code_text(code: &str, variant: &str) -> String {
    if variant.is_empty() {
        code.to_owned()
    } else {
        format!("{code} · {variant}")
    }
}

fn terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn category_icon(category: &str) -> &'static str {
    match category {
        "Shell" => "dashboard",
        "Utilities" => "construction",
        "Screen" => "zoom_in",
        "Media" => "music_note",
        "Window" => "select_window",
        "Workspace" => "view_carousel",
        "Session" => "power_settings_new",
        "Input" => "keyboard",
        "App" => "apps",
        _ => "keyboard_command_key",
    }
}

pub fn round_button(page: &Page, icon: &str, is_enabled: bool) -> RippleButton {
    let button = RippleButton::new(&page.theme);
    button.set_radius(ROUND_BUTTON as f64 / 2.0);
    button.set_size_request(ROUND_BUTTON, ROUND_BUTTON);
    button.set_valign(gtk4::Align::Center);
    let symbol = text::symbol(icon, ROUND_ICON);
    text::set_color(
        &symbol,
        if is_enabled {
            "colOnLayer2"
        } else {
            "colSubtext"
        },
    );
    button.set_content(&Centered::integral(&symbol), 0, 0);
    button.set_sensitive(is_enabled);
    button
}

struct Keyboard {
    page: Weak<Page>,
    options: Rc<HyprOptions>,
    catalog: Rc<RefCell<Catalog>>,
    rows: gtk4::Box,
    tips: RefCell<Vec<Box<dyn Any>>>,
    candidates: RefCell<Vec<Candidate>>,
    visible_indices: gtk4::StringList,
    query: RefCell<String>,
}

impl Keyboard {
    fn sources(&self) -> Vec<Source> {
        let codes = split(&self.options.text("input:kb_layout"));
        let variants = split(&self.options.text("input:kb_variant"));
        codes
            .into_iter()
            .enumerate()
            .map(|(index, code)| Source {
                code,
                variant: variants.get(index).cloned().unwrap_or_default(),
            })
            .collect()
    }

    fn apply(&self, sources: &[Source]) {
        let codes: Vec<&str> = sources.iter().map(|source| source.code.as_str()).collect();
        let variants: Vec<&str> = sources
            .iter()
            .map(|source| source.variant.as_str())
            .collect();
        self.options.set("input:kb_layout", &codes.join(","));
        self.options.set("input:kb_variant", &variants.join(","));
    }

    fn move_source(&self, index: usize, to: usize) {
        let mut sources = self.sources();
        if to >= sources.len() {
            return;
        }
        let source = sources.remove(index);
        sources.insert(to, source);
        self.apply(&sources);
    }

    fn remove_source(&self, index: usize) {
        let mut sources = self.sources();
        if sources.len() <= 1 {
            return;
        }
        sources.remove(index);
        self.apply(&sources);
    }

    fn show_sources(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        let sources = self.sources();
        let catalog = self.catalog.borrow();
        let mut tips: Vec<Box<dyn Any>> = Vec::new();
        for (index, source) in sources.iter().enumerate() {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, SOURCE_HEIGHT);
            let content = Row::new(SOURCE_SPACING);
            content.set_margin_start(SOURCE_START);
            content.set_margin_end(SOURCE_END);
            content.set_hexpand(true);
            let number = text::styled(&format!("{}", index + 1));
            text::set_color(&number, "colSubtext");
            content.append(&Centered::new(&number));
            let name = text::styled(&name_of(&catalog, &source.code, &source.variant));
            text::set_color(&name, "colOnLayer2");
            name.set_xalign(0.0);
            name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let name = Centered::filling_width(&name);
            name.set_hexpand(true);
            content.append(&name);
            let code = text::styled_sized(
                &code_text(&source.code, &source.variant),
                pixel_size::SMALLER,
            );
            text::set_color(&code, "colSubtext");
            content.append(&Centered::new(&code));
            let count = sources.len();
            for (icon, tip, is_enabled, target) in [
                (
                    "keyboard_arrow_up",
                    "Move up",
                    index > 0,
                    index.checked_sub(1),
                ),
                (
                    "keyboard_arrow_down",
                    "Move down",
                    index + 1 < count,
                    Some(index + 1),
                ),
                ("remove", "Remove", count > 1, None),
            ] {
                let button = round_button(&page, icon, is_enabled);
                button.connect_clicked({
                    let keyboard = Rc::downgrade(self);
                    move |_| {
                        let Some(keyboard) = keyboard.upgrade() else {
                            return;
                        };
                        match target {
                            Some(to) => keyboard.move_source(index, to),
                            None if icon == "remove" => keyboard.remove_source(index),
                            None => {}
                        }
                    }
                });
                tips.push(Box::new(page.unkept_tip(&button, &tr(tip))));
                content.append(&button);
            }
            card.append(&content);
            self.rows.append(&card);
        }
        self.tips.replace(tips);
    }

    fn filter(&self) {
        let catalog = self.catalog.borrow();
        let terms = terms(&self.query.borrow());
        let mut candidates = Vec::new();
        for layout in &catalog.layouts {
            candidates.push(Candidate {
                code: layout.code.clone(),
                variant: String::new(),
                name: layout.name.clone(),
            });
            for variant in &layout.variants {
                candidates.push(Candidate {
                    code: layout.code.clone(),
                    variant: variant.code.clone(),
                    name: variant.name.clone(),
                });
            }
        }
        candidates.retain(|entry| {
            let haystack =
                format!("{} {} {}", entry.name, entry.code, entry.variant).to_lowercase();
            terms.iter().all(|term| haystack.contains(term.as_str()))
        });
        let indices: Vec<String> = (0..candidates.len())
            .map(|index| index.to_string())
            .collect();
        let indices: Vec<&str> = indices.iter().map(String::as_str).collect();
        self.candidates.replace(candidates);
        self.visible_indices
            .splice(0, self.visible_indices.n_items(), &indices);
    }

    fn add(&self, candidate: &Candidate) -> bool {
        let mut sources = self.sources();
        if sources
            .iter()
            .any(|source| source.code == candidate.code && source.variant == candidate.variant)
        {
            return false;
        }
        sources.push(Source {
            code: candidate.code.clone(),
            variant: candidate.variant.clone(),
        });
        self.apply(&sources);
        true
    }
}

fn option_of(options: &HyprOptions, prefix: &str) -> String {
    let prefix = format!("{prefix}:");
    split(&options.text("input:kb_options"))
        .into_iter()
        .find(|option| option.starts_with(&prefix))
        .unwrap_or_default()
}

fn set_option(options: &Rc<HyprOptions>, prefix: &str, option: &str) {
    let start = format!("{prefix}:");
    let mut kb_options: Vec<String> = split(&options.text("input:kb_options"))
        .into_iter()
        .filter(|entry| !entry.starts_with(&start))
        .collect();
    if !option.is_empty() {
        kb_options.push(option.to_owned());
    }
    options.set("input:kb_options", &kb_options.join(","));
}

fn option_prefix(group: Option<&OptionGroup>) -> String {
    group
        .and_then(|group| group.options.first())
        .map(|option| option.code.split(':').next().unwrap_or("").to_owned())
        .unwrap_or_default()
}

pub(super) fn option_combo(
    page: &Page,
    parent: &gtk4::Box,
    (options, catalog): (&Rc<HyprOptions>, &Rc<RefCell<Catalog>>),
    (title, icon): (&str, &str),
    group_code: &'static str,
    none_label: &'static str,
) -> Rc<dyn Fn()> {
    let group = page.subsection(parent, &tr(title), "");
    let combo = page.combo(&group, icon);
    let values: Rc<RefCell<Vec<String>>> = Rc::default();
    let show: Rc<dyn Fn()> = Rc::new({
        let combo = Rc::downgrade(&combo);
        let options = Rc::downgrade(options);
        let catalog = Rc::downgrade(catalog);
        let values = values.clone();
        move || {
            let (Some(combo), Some(options), Some(catalog)) =
                (combo.upgrade(), options.upgrade(), catalog.upgrade())
            else {
                return;
            };
            let catalog = catalog.borrow();
            let group = catalog
                .option_groups
                .iter()
                .find(|group| group.code == group_code);
            let mut labels = vec![tr(none_label)];
            let mut codes = vec![String::new()];
            for option in group.map(|group| group.options.as_slice()).unwrap_or(&[]) {
                labels.push(option.name.clone());
                codes.push(option.code.clone());
            }
            let current_option = option_of(&options, &option_prefix(group));
            let index = codes
                .iter()
                .position(|code| *code == current_option)
                .unwrap_or(0);
            combo.set_items(&labels, index as i32);
            values.replace(codes);
        }
    });
    combo.connect_activated({
        let options = Rc::downgrade(options);
        let catalog = Rc::downgrade(catalog);
        move |index| {
            let (Some(options), Some(catalog)) = (options.upgrade(), catalog.upgrade()) else {
                return;
            };
            let prefix = option_prefix(
                catalog
                    .borrow()
                    .option_groups
                    .iter()
                    .find(|group| group.code == group_code),
            );
            if let Some(value) = values.borrow().get(index) {
                set_option(&options, &prefix, value);
            }
        }
    });
    show
}

fn chooser_list(
    page: &Page,
    keyboard: &Rc<Keyboard>,
    set_choosing: Rc<dyn Fn(bool)>,
) -> gtk4::Widget {
    let factory = gtk4::SignalListItemFactory::new();
    let theme = page.theme.clone();
    factory.connect_setup({
        let keyboard = Rc::downgrade(keyboard);
        move |_, item| {
            let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let button = RippleButton::new(&theme);
            button.set_radius(crate::ui::theme::rounding::SMALL as f64);
            button.set_size_request(-1, CANDIDATE_HEIGHT);
            button.set_margin_bottom(CANDIDATE_GAP);
            let row = Row::new(SOURCE_SPACING);
            let name = text::styled("");
            text::set_color(&name, "colOnLayer2");
            name.set_xalign(0.0);
            name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let name_box = Centered::filling_width(&name);
            name_box.set_hexpand(true);
            name_box.set_margin_start(CANDIDATE_SIDE);
            row.append(&name_box);
            let code = text::styled_sized("", pixel_size::SMALLER);
            text::set_color(&code, "colSubtext");
            let code_box = Centered::new(&code);
            code_box.set_margin_end(CANDIDATE_SIDE);
            row.append(&code_box);
            button.set_content(&row, CANDIDATE_PADDING, 0);
            let list_item = item.downgrade();
            let keyboard = keyboard.clone();
            let set_choosing = set_choosing.clone();
            button.connect_clicked(move |_| {
                let (Some(list_item), Some(keyboard)) = (list_item.upgrade(), keyboard.upgrade())
                else {
                    return;
                };
                let Some(index) = list_item
                    .item()
                    .and_downcast::<gtk4::StringObject>()
                    .and_then(|index| index.string().parse::<usize>().ok())
                else {
                    return;
                };
                let candidate = keyboard.candidates.borrow().get(index).cloned();
                if let Some(candidate) = candidate
                    && keyboard.add(&candidate)
                {
                    set_choosing(false);
                }
            });
            item.set_child(Some(&button));
        }
    });
    factory.connect_bind({
        let keyboard = Rc::downgrade(keyboard);
        move |_, item| {
            let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let (Some(keyboard), Some(button)) = (keyboard.upgrade(), item.child()) else {
                return;
            };
            let Some(index) = item
                .item()
                .and_downcast::<gtk4::StringObject>()
                .and_then(|index| index.string().parse::<usize>().ok())
            else {
                return;
            };
            let Some(candidate) = keyboard.candidates.borrow().get(index).cloned() else {
                return;
            };
            let labels: Vec<gtk4::Label> = {
                let mut labels = Vec::new();
                let mut stack = vec![button];
                while let Some(widget) = stack.pop() {
                    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
                        labels.push(label.clone());
                    }
                    let mut child = widget.last_child();
                    while let Some(current) = child {
                        child = current.prev_sibling();
                        stack.push(current);
                    }
                }
                labels
            };
            if let [name, code] = labels.as_slice() {
                name.set_text(&candidate.name);
                code.set_text(&code_text(&candidate.code, &candidate.variant));
            }
        }
    });
    let list = gtk4::ListView::new(
        Some(gtk4::NoSelection::new(Some(
            keyboard.visible_indices.clone(),
        ))),
        Some(factory),
    );
    list.add_css_class("plain-list");
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_child(Some(&list));
    crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);
    keyboard.visible_indices.connect_items_changed({
        let scroll = scroll.downgrade();
        move |_, _, _, _| {
            if let Some(scroll) = scroll.upgrade() {
                scroll.vadjustment().set_value(0.0);
            }
        }
    });
    scroll.set_margin_start(CHOOSER_MARGIN);
    scroll.set_margin_end(CHOOSER_MARGIN);
    scroll.set_margin_top(CHOOSER_MARGIN);
    scroll.set_margin_bottom(CHOOSER_MARGIN);
    scroll.set_vexpand(true);
    let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    frame.add_css_class("settings-row-card");
    frame.set_overflow(gtk4::Overflow::Hidden);
    frame.set_size_request(-1, CHOOSER_HEIGHT);
    frame.append(&scroll);
    frame.upcast()
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(Area::Keyboard, &OPTIONS);

    let sources_section = page.section("keyboard", &tr("Input Sources"));
    let list_section = page.subsection(
        &sources_section,
        &tr("Keyboard layouts, in the order they are cycled through"),
        "",
    );
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    list_section.append(&rows);
    let keyboard = Rc::new(Keyboard {
        page: Rc::downgrade(&page),
        options: options.clone(),
        catalog: Rc::default(),
        rows,
        tips: RefCell::new(Vec::new()),
        candidates: RefCell::new(Vec::new()),
        visible_indices: gtk4::StringList::new(&[]),
        query: RefCell::new(String::new()),
    });

    let is_choosing = Rc::new(Cell::new(false));
    let chooser = page.subsection(&sources_section, &tr("Add an input source"), "");
    let chooser_root = Page::subsection_root(&chooser);
    chooser_root.set_visible(false);
    let toggle_label: Rc<RefCell<Option<(gtk4::Label, gtk4::Label)>>> = Rc::default();
    let search = TextField::new(&page.theme, Style::Outlined, &tr("Search layouts"));
    search.root.set_hexpand(true);
    let set_choosing: Rc<dyn Fn(bool)> = Rc::new({
        let is_choosing = is_choosing.clone();
        let chooser_root = chooser_root.downgrade();
        let toggle_label = toggle_label.clone();
        let search = Rc::downgrade(&search);
        move |is_on| {
            is_choosing.set(is_on);
            if let Some(chooser_root) = chooser_root.upgrade() {
                chooser_root.set_visible(is_on);
            }
            if let Some(field) = search.upgrade() {
                field.set_text("");
            }
            if let Some((symbol, name)) = toggle_label.borrow().as_ref() {
                symbol.set_text(if is_on { "close" } else { "add" });
                name.set_text(&if is_on {
                    tr("Cancel")
                } else {
                    tr("Add input source")
                });
            }
        }
    });
    let (toggle_button, toggle_name) = page.icon_button("add", true, &tr("Add input source"), {
        let is_choosing = is_choosing.clone();
        let set_choosing = set_choosing.clone();
        move || set_choosing(!is_choosing.get())
    });
    toggle_button.set_margin_top(ADD_TOP);
    list_section.append(&toggle_button);
    let toggle_symbol = toggle_button
        .first_child()
        .and_then(|row| row.first_child())
        .and_then(|centered| centered.first_child())
        .and_downcast::<gtk4::Label>();
    if let Some(symbol) = toggle_symbol {
        toggle_label.replace(Some((symbol, toggle_name)));
    }
    chooser.append(&search.root);
    search.connect_changed({
        let keyboard = Rc::downgrade(&keyboard);
        let search = Rc::downgrade(&search);
        move || {
            let (Some(keyboard), Some(search)) = (keyboard.upgrade(), search.upgrade()) else {
                return;
            };
            keyboard.query.replace(search.text());
            keyboard.filter();
        }
    });
    chooser.append(&chooser_list(&page, &keyboard, set_choosing.clone()));
    page.keep(search);

    let switching_section = page.section("swap_horiz", &tr("Input Source Switching"));
    let show_switch_option = option_combo(
        &page,
        &switching_section,
        (&options, &keyboard.catalog),
        ("Switch between layouts with", "swap_horiz"),
        "grp",
        "Only the shell shortcut",
    );
    hyprrows::switch(
        &page,
        &switching_section,
        &options,
        "pin",
        &tr("Num Lock when the session starts"),
        "input:numlock_by_default",
    );

    page.link_row(
        &switching_section,
        "keyboard_command_key",
        &tr("Keyboard options"),
        &tr("Compose, alternate characters, Caps Lock, Ctrl, Alt and Super"),
        context.subpage_opener("keyoptions"),
    );

    let shortcuts_section = page.section("shortcut", &tr("Keyboard Shortcuts"));
    let by_symbol_switch = hyprrows::switch(
        &page,
        &shortcuts_section,
        &options,
        "language",
        &tr("Shortcuts follow the symbol on the key"),
        "input:resolve_binds_by_sym",
    );
    page.tip(
        &by_symbol_switch.button,
        &tr(
            "On: a shortcut is the letter it types, so it moves with the layout.\nOff: a shortcut is the place on the keyboard, so it stays put in any layout.",
        ),
    );
    let shortcut_search = TextField::new(&page.theme, Style::Outlined, &tr("Search shortcuts"));
    shortcut_search.root.set_hexpand(true);
    shortcuts_section.append(&shortcut_search.root);
    let links = gtk4::Box::new(gtk4::Orientation::Vertical, shortcuts_section.spacing());
    shortcuts_section.append(&links);
    let results = gtk4::Box::new(gtk4::Orientation::Vertical, RESULTS_SPACING);
    shortcuts_section.append(&results);
    let editor = shortcuts::Editor::new(context, &page);
    let open = Rc::new(context.subpage_opener_with("shortcuts"));
    let show_binds = Rc::new({
        let page = Rc::downgrade(&page);
        let search = Rc::downgrade(&shortcut_search);
        let editor = editor.clone();
        move || {
            let (Some(page), Some(search)) = (page.upgrade(), search.upgrade()) else {
                return;
            };
            for parent in [&links, &results] {
                while let Some(child) = parent.first_child() {
                    parent.remove(&child);
                }
            }
            editor.clear_tips();
            let terms = terms(&search.text());
            links.set_visible(terms.is_empty());
            results.set_visible(!terms.is_empty());
            let all_shortcuts = editor.shortcuts();
            for category in editor.categories() {
                let members: Vec<&Shortcut> = all_shortcuts
                    .iter()
                    .filter(|shortcut| shortcut.category() == category)
                    .collect();
                if terms.is_empty() {
                    let labels: Vec<&str> =
                        members.iter().map(|shortcut| shortcut.label()).collect();
                    let open = open.clone();
                    let target = category.clone();
                    page.link_row(
                        &links,
                        category_icon(&category),
                        &category,
                        &labels.join(", "),
                        move || open(&target),
                    );
                    continue;
                }
                let matching_shortcuts: Vec<&Shortcut> = members
                    .into_iter()
                    .filter(|shortcut| {
                        let keys: Vec<&str> =
                            shortcut.keys.iter().flatten().map(String::as_str).collect();
                        let haystack =
                            format!("{} {}", shortcut.label(), keys.join(" ")).to_lowercase();
                        terms.iter().all(|term| haystack.contains(term.as_str()))
                    })
                    .collect();
                if matching_shortcuts.is_empty() {
                    continue;
                }
                let (group, _) = page.unkept_subsection(&results, &category, "");
                for shortcut in matching_shortcuts {
                    group.append(&editor.row(shortcut));
                }
            }
        }
    });
    show_binds();
    shortcut_search.connect_changed({
        let show_binds = show_binds.clone();
        move || show_binds()
    });
    page.keep(shortcut_search);
    page.keep(context.services.events.subscribe({
        let show_binds = show_binds.clone();
        let editor = editor.clone();
        move |event, _| {
            if event == "configreloaded" {
                editor.reload();
                show_binds();
            }
        }
    }));
    page.keep(editor);

    let show_sources_and_switching = {
        let keyboard = Rc::downgrade(&keyboard);
        move || {
            if let Some(keyboard) = keyboard.upgrade() {
                keyboard.show_sources();
            }
            show_switch_option();
        }
    };
    let show_sources_and_switching = Rc::new(show_sources_and_switching);
    show_sources_and_switching();
    options.connect_changed({
        let show_sources_and_switching = show_sources_and_switching.clone();
        move || show_sources_and_switching()
    });
    glib::spawn_future_local({
        let keyboard = Rc::downgrade(&keyboard);
        async move {
            let Ok(catalog) = gio::spawn_blocking(xkbregistry::load).await else {
                return;
            };
            if let Some(keyboard) = keyboard.upgrade() {
                keyboard.catalog.replace(catalog);
                keyboard.filter();
                show_sources_and_switching();
            }
        }
    });

    page.keep(keyboard);
    page.keep(options);
    page
}
