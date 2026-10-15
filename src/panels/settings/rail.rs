use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::tr;
use crate::core::{config, process};
use crate::panels::settings::index::{Hit, Index};
use crate::panels::settings::pages::PAGES;
use crate::ui::anim::{self, EXPRESSIVE_EFFECTS, EXPRESSIVE_FAST, Tween};
use crate::ui::theme::{SharedTheme, Theme, pixel_size, rounding, transparentize};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::input::textfield::{Style, TextField};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::fixedwidth::FixedWidth;
use crate::ui::widgets::layout::group::pointer_cursor;
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};

const BASE: i32 = 56;
const HIGHLIGHT_HEIGHT: i32 = 32;
const GROUP_SPACING: i32 = 13;
const SEPARATOR_MARGIN: i32 = 8;
const LABEL_GAP: i32 = 20;
const MIN_WIDTH: i32 = 230;
const SPACING: i32 = 10;
const TOGGLE_SIZE: i32 = 40;
const TOGGLE_MARGIN: i32 = 8;
const TOGGLE_ICON: f64 = 24.0;
const FAB_RADIUS: f64 = 16.0;
const FAB_ICON: f64 = 26.0;
const FAB_TEXT_GAP: i32 = 5;
const FIELD_MARGIN: i32 = 4;
const TAB_ICON: f64 = 24.0;
const TAB_TEXT: i32 = 14;
const WIDTH_MILLIS: f64 = 200.0;
const HIGHLIGHT_MILLIS: f64 = 350.0;
const FILL_MILLIS: f64 = 200.0;
const COPY_NOTICE_DURATION: Duration = Duration::from_millis(1500);
const EXPAND_ABOVE_WIDTH: i32 = 900;
const RESULT_LIMIT: usize = 60;
const RESULT_SPACING: i32 = 2;
const RESULT_PADDING: i32 = 12;
const RESULT_HEIGHT: i32 = 48;

type Selected = Rc<dyn Fn(usize)>;
type Found = Rc<dyn Fn(&Hit)>;

const FAB: Look = Look {
    background: |theme| theme.colors.col_primary_container,
    hover: |theme| theme.colors.col_primary_container_hover,
    toggled: |theme| theme.colors.col_primary_container,
    toggled_hover: |theme| theme.colors.col_primary_container_hover,
    ripple: |theme| theme.colors.col_primary_container_active,
    ripple_toggled: |theme| theme.colors.col_primary_container_active,
};

struct Tab {
    page: usize,
    outer: gtk4::Box,
    separator: gtk4::Widget,
    button: gtk4::Overlay,
    background: Paint,
    icon: gtk4::Label,
    label: gtk4::Label,
    visual_width: i32,
    fade: anim::Fade,
    fill: Cell<Tween>,
    last_fill: Cell<f64>,
    is_hovered: Cell<bool>,
    is_down: Cell<bool>,
    is_toggled: Cell<bool>,
    is_expanded: Cell<bool>,
    is_ticking: Cell<bool>,
    tip: Rc<Tooltip>,
    theme: SharedTheme,
}

pub struct Rail {
    pub root: FixedWidth,
    theme: SharedTheme,
    is_expanded: Cell<bool>,
    has_user_choice: Cell<bool>,
    width: Cell<Tween>,
    is_ticking: Cell<bool>,
    toggle_icon: gtk4::Label,
    toggle_icon_holder: Centered,
    fab: RippleButton,
    fab_icon: gtk4::Label,
    fab_text: gtk4::Label,
    copy_notice_timer: RefCell<Option<glib::SourceId>>,
    search: Rc<TextField>,
    scroller: gtk4::ScrolledWindow,
    list: gtk4::Box,
    results_scroller: gtk4::ScrolledWindow,
    results: gtk4::Box,
    hits: RefCell<Vec<Hit>>,
    index: RefCell<Option<Index>>,
    highlight: Paint,
    highlight_y: Rc<anim::Motion>,
    tabs: Vec<Rc<Tab>>,
    current_page: Cell<usize>,
    should_reveal_current: Cell<bool>,
    on_selected: RefCell<Option<Selected>>,
    on_found: RefCell<Option<Found>>,
    _tips: Vec<Rc<Tooltip>>,
}

impl Rail {
    pub fn new(theme: &SharedTheme) -> Rc<Self> {
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);

        let toggle_button = RippleButton::new(theme);
        toggle_button.set_radius(rounding::FULL as f64);
        toggle_button.set_size_request(TOGGLE_SIZE, TOGGLE_SIZE);
        toggle_button.set_halign(gtk4::Align::Start);
        toggle_button.set_margin_start(TOGGLE_MARGIN);
        let toggle_icon = text::symbol("menu_open", TOGGLE_ICON);
        text::set_color(&toggle_icon, "colOnLayer1");
        let toggle_icon_holder = Centered::new(&toggle_icon);
        toggle_button.set_content(&toggle_icon_holder, 0, 0);
        column.append(&toggle_button);

        let fab = RippleButton::new(theme);
        fab.set_look(FAB);
        fab.set_radius(FAB_RADIUS);
        fab.set_size_request(BASE, BASE);
        fab.set_overflow(gtk4::Overflow::Hidden);
        let fab_icon = text::symbol("edit", FAB_ICON);
        text::set_color(&fab_icon, "colOnPrimaryContainer");
        let fab_text = text::styled_sized(&tr("Config file"), TAB_TEXT);
        text::set_color(&fab_text, "colOnPrimaryContainer");
        fab_text.set_margin_start(FAB_TEXT_GAP);
        let fab_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        fab_row.set_halign(gtk4::Align::Start);
        fab_row.set_valign(gtk4::Align::Center);
        let icon_width = fab_icon.measure(gtk4::Orientation::Horizontal, -1).1;
        fab_icon.set_margin_start((BASE - icon_width) / 2);
        fab_row.append(&fab_icon);
        fab_row.append(&fab_text);
        fab.set_content(&fab_row, 0, 0);
        column.append(&fab);

        let search = TextField::new(theme, Style::Outlined, &tr("Search"));
        search.root.set_margin_start(FIELD_MARGIN);
        search.root.set_margin_end(FIELD_MARGIN);
        column.append(&search.root);

        let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let highlight = Paint::new(|_, _, _| {});
        let highlight_y = anim::Motion::new(&highlight, 0.0, HIGHLIGHT_MILLIS, EXPRESSIVE_FAST);
        let stack = gtk4::Overlay::new();
        stack.set_child(Some(&highlight));
        stack.add_overlay(&list);
        stack.set_measure_overlay(&list, true);
        let scroller = gtk4::ScrolledWindow::new();
        scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&stack));
        crate::ui::widgets::input::flickable::follow_scroll_settings(&scroller);
        column.append(&scroller);

        let results = gtk4::Box::new(gtk4::Orientation::Vertical, RESULT_SPACING);
        let results_scroller = gtk4::ScrolledWindow::new();
        results_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        results_scroller.set_vexpand(true);
        results_scroller.set_child(Some(&results));
        results_scroller.set_visible(false);
        crate::ui::widgets::input::flickable::follow_scroll_settings(&results_scroller);
        column.append(&results_scroller);

        let tabs: Vec<Rc<Tab>> = (0..PAGES.len()).map(|page| Tab::new(theme, page)).collect();
        for tab in &tabs {
            list.append(&tab.outer);
        }
        let widest = tabs.iter().map(|tab| tab.visual_width).max().unwrap_or(0);

        let root = FixedWidth::new(expanded_width(widest));
        root.set_child(&column);

        let fab_tip = Tooltip::new(&fab, theme, tooltip::Kind::Styled);
        fab_tip.set_text(&tr(
            "Open the shell config file\nAlternatively right-click to copy path",
        ));
        tooltip::hover_delay(&fab, &fab_tip, 0);

        let rail = Rc::new(Rail {
            root,
            theme: theme.clone(),
            is_expanded: Cell::new(true),
            has_user_choice: Cell::new(false),
            width: Cell::new(Tween::new(
                expanded_width(widest) as f64,
                WIDTH_MILLIS,
                EXPRESSIVE_EFFECTS,
            )),
            is_ticking: Cell::new(false),
            toggle_icon,
            toggle_icon_holder,
            fab: fab.clone(),
            fab_icon,
            fab_text,
            copy_notice_timer: RefCell::new(None),
            search: search.clone(),
            scroller,
            list,
            results_scroller,
            results,
            hits: RefCell::new(Vec::new()),
            index: RefCell::new(None),
            highlight: highlight.clone(),
            highlight_y,
            tabs,
            current_page: Cell::new(0),
            should_reveal_current: Cell::new(false),
            on_selected: RefCell::new(None),
            on_found: RefCell::new(None),
            _tips: vec![fab_tip],
        });

        toggle_button.connect_down({
            let rail = Rc::downgrade(&rail);
            move || {
                if let Some(rail) = rail.upgrade() {
                    rail.has_user_choice.set(true);
                    rail.set_expanded(!rail.is_expanded.get());
                }
            }
        });
        fab.connect_down(|| {
            process::launch(&["xdg-open", &config::config_path().to_string_lossy()]);
        });
        fab.connect_alt({
            let rail = Rc::downgrade(&rail);
            move |_| {
                if let Some(rail) = rail.upgrade() {
                    rail.copy_path();
                }
            }
        });
        search.connect_changed({
            let rail = Rc::downgrade(&rail);
            move || {
                if let Some(rail) = rail.upgrade() {
                    rail.filter(&rail.search.text());
                }
            }
        });
        search.connect_accepted({
            let rail = Rc::downgrade(&rail);
            move || {
                let Some(rail) = rail.upgrade() else {
                    return;
                };
                let first_hit = rail.hits.borrow().first().cloned();
                if let Some(hit) = first_hit {
                    rail.choose(&hit);
                }
            }
        });
        for tab in &rail.tabs {
            let press_gesture = gtk4::GestureClick::new();
            press_gesture.connect_pressed({
                let rail = Rc::downgrade(&rail);
                let page = tab.page;
                move |_, _, _, _| {
                    let Some(rail) = rail.upgrade() else {
                        return;
                    };
                    let on_selected = rail.on_selected.borrow().clone();
                    if let Some(on_selected) = on_selected {
                        on_selected(page);
                    }
                }
            });
            tab.button.add_controller(press_gesture);
        }
        highlight.set_draw({
            let rail = Rc::downgrade(&rail);
            move |snapshot, _, _| {
                if let Some(rail) = rail.upgrade() {
                    rail.draw_highlight(snapshot);
                }
            }
        });
        rail.scroller.vadjustment().connect_changed({
            let rail = Rc::downgrade(&rail);
            move |_| {
                let rail = rail.clone();
                glib::idle_add_local_once(move || {
                    if let Some(rail) = rail.upgrade() {
                        rail.reveal_current();
                    }
                });
            }
        });
        rail.set_current_page(0);
        rail
    }

    pub fn connect_selected(&self, action: impl Fn(usize) + 'static) {
        self.on_selected.replace(Some(Rc::new(action)));
    }

    pub fn connect_found(&self, action: impl Fn(&Hit) + 'static) {
        self.on_found.replace(Some(Rc::new(action)));
    }

    fn choose(&self, hit: &Hit) {
        let on_found = self.on_found.borrow().clone();
        if let Some(on_found) = on_found {
            on_found(hit);
        }
    }

    pub fn current_page(&self) -> usize {
        self.current_page.get()
    }

    pub fn set_current_page(&self, page: usize) {
        self.current_page.set(page);
        for tab in &self.tabs {
            tab.set_toggled(tab.page == page);
        }
        self.highlight.queue_draw();
        self.should_reveal_current.set(true);
        self.reveal_current();
    }

    pub fn type_into_search(self: &Rc<Self>, key_controller: &gtk4::EventControllerKey) -> bool {
        self.set_expanded(true);
        self.search.type_from(key_controller)
    }

    pub fn follow_window_width(self: &Rc<Self>, width: i32) {
        if self.has_user_choice.get() {
            return;
        }
        self.set_expanded(width > EXPAND_ABOVE_WIDTH);
    }

    fn set_expanded(self: &Rc<Self>, is_expanded: bool) {
        if self.is_expanded.replace(is_expanded) == is_expanded {
            return;
        }
        self.toggle_icon
            .set_text(if is_expanded { "menu_open" } else { "menu" });
        self.toggle_icon_holder
            .rotate_to(if is_expanded { 0.0 } else { -180.0 });
        self.search.root.set_visible(is_expanded);
        self.fab_text.set_visible(is_expanded);
        if !is_expanded {
            self.search.set_text("");
        }
        self.fab.set_halign(if is_expanded {
            gtk4::Align::Fill
        } else {
            gtk4::Align::Start
        });
        for tab in &self.tabs {
            tab.set_expanded(is_expanded);
        }
        let widest = self
            .tabs
            .iter()
            .map(|tab| tab.visual_width)
            .max()
            .unwrap_or(0);
        let target = if is_expanded {
            expanded_width(widest)
        } else {
            BASE
        };
        self.animate_width(target);
        self.highlight.queue_draw();
    }

    fn animate_width(self: &Rc<Self>, target: i32) {
        let root = &self.root;
        let now = root
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let mut tween = self.width.get();
        if !root.is_mapped() {
            tween.jump(target as f64);
            self.width.set(tween);
            root.set_width(target);
            return;
        }
        tween.retarget(target as f64, now);
        self.width.set(tween);
        if self.is_ticking.replace(true) {
            return;
        }
        let rail = Rc::downgrade(self);
        root.add_tick_callback(move |root, clock| {
            let Some(rail) = rail.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let tween = rail.width.get();
            let now = clock.frame_time();
            root.set_width(tween.value(now).round() as i32);
            rail.highlight.queue_draw();
            if tween.is_running(now) {
                return glib::ControlFlow::Continue;
            }
            rail.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn filter(self: &Rc<Self>, query: &str) {
        let is_searching = !query.trim().is_empty();
        self.scroller.set_visible(!is_searching);
        self.results_scroller.set_visible(is_searching);
        while let Some(child) = self.results.first_child() {
            self.results.remove(&child);
        }
        let hits = if is_searching {
            self.index
                .borrow_mut()
                .get_or_insert_with(Index::new)
                .search(query)
        } else {
            Vec::new()
        };
        if is_searching && hits.is_empty() {
            let empty_label = text::styled_sized(&tr("No settings found"), TAB_TEXT);
            text::set_color(&empty_label, "colSubtext");
            empty_label.set_margin_top(RESULT_PADDING);
            self.results.append(&empty_label);
        }
        for hit in hits.iter().take(RESULT_LIMIT) {
            self.results.append(&self.result_row(hit));
        }
        self.results_scroller.vadjustment().set_value(0.0);
        self.hits.replace(hits);
        if !is_searching {
            self.highlight.queue_draw();
        }
    }

    fn result_row(self: &Rc<Self>, hit: &Hit) -> RippleButton {
        let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        lines.set_valign(gtk4::Align::Center);
        let title = text::styled_sized(&hit.title, TAB_TEXT);
        text::set_color(&title, "colOnLayer1");
        title.set_xalign(0.0);
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centered::filling_width(&title));
        if !hit.trail.is_empty() {
            let trail = text::styled_sized(&hit.trail, pixel_size::SMALLER);
            text::set_color(&trail, "colSubtext");
            trail.set_xalign(0.0);
            trail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centered::filling_width(&trail));
        }
        let button = RippleButton::new(&self.theme);
        button.set_radius(rounding::SMALL as f64);
        button.set_size_request(-1, RESULT_HEIGHT);
        button.set_content(&lines, RESULT_PADDING, 0);
        button.connect_clicked({
            let rail = Rc::downgrade(self);
            let hit = hit.clone();
            move |_| {
                if let Some(rail) = rail.upgrade() {
                    rail.choose(&hit);
                }
            }
        });
        button
    }

    fn copy_path(&self) {
        self.root
            .clipboard()
            .set_text(&config::config_path().to_string_lossy());
        self.fab_icon.set_text("check");
        self.fab_text.set_text(&tr("Path copied"));
        if let Some(timer) = self.copy_notice_timer.take() {
            timer.remove();
        }
        let icon = self.fab_icon.clone();
        let label = self.fab_text.clone();
        let timer = glib::timeout_add_local_once(COPY_NOTICE_DURATION, move || {
            icon.set_text("edit");
            label.set_text(&tr("Config file"));
        });
        self.copy_notice_timer.replace(Some(timer));
    }

    fn reveal_current(&self) {
        if !self.should_reveal_current.get() {
            return;
        }
        let tab = &self.tabs[self.current_page.get()];
        let adjustment = self.scroller.vadjustment();
        let Some(bounds) = tab.button.compute_bounds(&self.list) else {
            return;
        };
        if adjustment.page_size() <= 0.0 {
            return;
        }
        self.should_reveal_current.set(false);
        let group_spacing = if tab.separator.is_visible() {
            GROUP_SPACING as f64
        } else {
            0.0
        };
        let top = bounds.y() as f64 - group_spacing;
        let bottom = (bounds.y() + bounds.height()) as f64;
        if top < adjustment.value() {
            adjustment.set_value(top);
        } else if bottom > adjustment.value() + adjustment.page_size() {
            adjustment.set_value(bottom - adjustment.page_size());
        }
    }

    fn draw_highlight(&self, snapshot: &gtk4::Snapshot) {
        let tab = &self.tabs[self.current_page.get()];
        let Some(bounds) = tab.button.compute_bounds(&self.list) else {
            return;
        };
        let is_expanded = self.is_expanded.get();
        let offset = if is_expanded {
            0.0
        } else {
            ((BASE - HIGHLIGHT_HEIGHT) / 2) as f64
        };
        let target = bounds.y() as f64 + offset;
        if (self.highlight_y.get() - target).abs() > f64::EPSILON && !self.highlight_y.is_running()
        {
            let motion = self.highlight_y.clone();
            glib::idle_add_local_once(move || motion.to(target));
        }
        let height = if is_expanded { BASE } else { HIGHLIGHT_HEIGHT } as f32;
        let width = if is_expanded {
            (bounds.width() as i32).max(tab.visual_width)
        } else {
            BASE
        } as f32;
        let rect = graphene::Rect::new(0.0, self.highlight_y.get() as f32, width, height);
        let theme = self.theme.borrow();
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(rect, height.min(width) / 2.0));
        snapshot.append_color(&theme.colors.col_secondary_container, &rect);
        snapshot.pop();
    }
}

fn expanded_width(widest: i32) -> i32 {
    widest.max(MIN_WIDTH)
}

impl Tab {
    fn new(theme: &SharedTheme, page: usize) -> Rc<Self> {
        let info = &PAGES[page];
        let outer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let separator = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        separator.add_css_class("settings-rail-separator");
        separator.set_size_request(-1, 1);
        separator.set_margin_start(SEPARATOR_MARGIN);
        separator.set_margin_end(SEPARATOR_MARGIN);
        separator.set_margin_top((GROUP_SPACING - 1) / 2);
        separator.set_margin_bottom(GROUP_SPACING - 1 - (GROUP_SPACING - 1) / 2);
        separator.set_visible(info.starts_group);
        outer.append(&separator);

        let icon = text::symbol(info.icon, TAB_ICON);
        let icon_box = Centered::integral(&icon);
        icon_box.set_size_request(BASE, HIGHLIGHT_HEIGHT);
        icon_box.set_valign(gtk4::Align::Center);
        let name = tr(info.name);
        let label = text::styled_sized(&name, TAB_TEXT);
        text::set_color(&label, "colOnLayer1");
        label.set_valign(gtk4::Align::Center);
        let visual_width = BASE + LABEL_GAP + label.measure(gtk4::Orientation::Horizontal, -1).1;
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        content.set_halign(gtk4::Align::Start);
        content.append(&icon_box);
        content.append(&label);

        let background = Paint::new(|_, _, _| {});
        let button = gtk4::Overlay::new();
        button.set_child(Some(&background));
        button.add_overlay(&content);
        button.set_measure_overlay(&content, true);
        button.set_size_request(-1, BASE);
        outer.append(&button);
        pointer_cursor(button.upcast_ref());

        let tip = Tooltip::new(&button, theme, tooltip::Kind::Styled);
        tip.set_text(&name);

        let tab = Rc::new(Tab {
            page,
            outer,
            separator: separator.upcast(),
            button: button.clone(),
            background: background.clone(),
            icon,
            label,
            visual_width,
            fade: anim::Fade::new(),
            fill: Cell::new(Tween::new(0.0, FILL_MILLIS, EXPRESSIVE_EFFECTS)),
            last_fill: Cell::new(f64::NAN),
            is_hovered: Cell::new(false),
            is_down: Cell::new(false),
            is_toggled: Cell::new(false),
            is_expanded: Cell::new(true),
            is_ticking: Cell::new(false),
            tip,
            theme: theme.clone(),
        });

        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_enter({
            let tab = Rc::downgrade(&tab);
            move |_, _, _| {
                if let Some(tab) = tab.upgrade() {
                    tab.is_hovered.set(true);
                    tab.tip.show(!tab.is_expanded.get());
                    tab.restyle(true);
                }
            }
        });
        motion_controller.connect_leave({
            let tab = Rc::downgrade(&tab);
            move |_| {
                if let Some(tab) = tab.upgrade() {
                    tab.is_hovered.set(false);
                    tab.tip.show(false);
                    tab.restyle(true);
                }
            }
        });
        button.add_controller(motion_controller);
        let click = gtk4::GestureClick::new();
        click.connect_pressed({
            let tab = Rc::downgrade(&tab);
            move |_, _, _, _| {
                if let Some(tab) = tab.upgrade() {
                    tab.is_down.set(true);
                    tab.restyle(true);
                }
            }
        });
        click.connect_released({
            let tab = Rc::downgrade(&tab);
            move |_, _, _, _| {
                if let Some(tab) = tab.upgrade() {
                    tab.is_down.set(false);
                    tab.restyle(true);
                }
            }
        });
        click.connect_cancel({
            let tab = Rc::downgrade(&tab);
            move |_, _| {
                if let Some(tab) = tab.upgrade() {
                    tab.is_down.set(false);
                    tab.restyle(true);
                }
            }
        });
        button.add_controller(click);

        background.set_draw({
            let tab = Rc::downgrade(&tab);
            move |snapshot, width, height| {
                if let Some(tab) = tab.upgrade() {
                    tab.draw(snapshot, width, height);
                }
            }
        });
        tab.restyle(false);
        tab
    }

    fn set_toggled(self: &Rc<Self>, is_toggled: bool) {
        if self.is_toggled.replace(is_toggled) == is_toggled {
            return;
        }
        self.restyle(true);
    }

    fn set_expanded(&self, is_expanded: bool) {
        self.is_expanded.set(is_expanded);
        self.label.set_visible(is_expanded);
        if is_expanded {
            self.tip.show(false);
        }
        self.background.queue_draw();
    }

    fn restyle(self: &Rc<Self>, should_animate: bool) {
        let is_toggled = self.is_toggled.get();
        self.icon.add_css_class("color-fade");
        text::set_color(
            &self.icon,
            if is_toggled {
                "m3onSecondaryContainer"
            } else {
                "colOnLayer1"
            },
        );
        let target = self.target(&self.theme.borrow());
        let now = self
            .background
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let should_animate = should_animate && self.background.is_mapped();
        self.fade.retarget(target, now, should_animate);
        let mut fill = self.fill.get();
        if should_animate {
            fill.retarget(if is_toggled { 1.0 } else { 0.0 }, now);
        } else {
            fill.jump(if is_toggled { 1.0 } else { 0.0 });
        }
        self.fill.set(fill);
        self.paint_fill(now);
        self.background.queue_draw();
        if self.is_ticking.replace(true) {
            return;
        }
        let tab = Rc::downgrade(self);
        self.background.add_tick_callback(move |background, clock| {
            let Some(tab) = tab.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            background.queue_draw();
            tab.paint_fill(now);
            if tab.fade.is_running(now) || tab.fill.get().is_running(now) {
                return glib::ControlFlow::Continue;
            }
            tab.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn paint_fill(&self, now: i64) {
        let fill = (self.fill.get().value(now) * 10.0).round() / 10.0;
        if self.last_fill.replace(fill) == fill {
            return;
        }
        text::set_symbol_font_weighted(&self.icon, TAB_ICON, fill, 400.0);
    }

    fn target(&self, theme: &Theme) -> RGBA {
        if self.is_toggled.get() {
            return transparentize(theme.colors.col_secondary_container, 1.0);
        }
        if self.is_down.get() {
            return theme.colors.col_layer1_active;
        }
        if self.is_hovered.get() {
            return theme.colors.col_layer1_hover;
        }
        transparentize(theme.colors.col_layer1_hover, 1.0)
    }

    fn draw(&self, snapshot: &gtk4::Snapshot, width: f32, height: f32) {
        let now = self
            .background
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let Some(color) = self.fade.value(now) else {
            return;
        };
        let rect = if self.is_expanded.get() {
            graphene::Rect::new(0.0, 0.0, width.max(self.visual_width as f32), height)
        } else {
            let top = ((height - HIGHLIGHT_HEIGHT as f32) / 2.0).round();
            graphene::Rect::new(0.0, top, BASE as f32, HIGHLIGHT_HEIGHT as f32)
        };
        let radius = rect.width().min(rect.height()) / 2.0;
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(rect, radius));
        snapshot.append_color(&color, &rect);
        snapshot.pop();
    }
}
