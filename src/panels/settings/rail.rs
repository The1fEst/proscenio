use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::{config, process};
use crate::panels::settings::index::{self, Hit};
use crate::panels::settings::pages::PAGES;
use crate::ui::anim::{self, EXPRESSIVE_EFFECTS, EXPRESSIVE_FAST, Tween};
use crate::ui::theme::{SharedTheme, Theme, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::fixedwidth::FixedWidth;
use crate::ui::widgets::group::pointer_cursor;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::{Style, TextField};
use crate::ui::widgets::tooltip::{self, Tooltip};

const BASE: i32 = 56;
const HIGHLIGHT_HEIGHT: i32 = 32;
const GROUP_SPACING: i32 = 13;
const SEPARATOR_MARGIN: i32 = 8;
const LABEL_GAP: i32 = 20;
const MIN_WIDTH: i32 = 150;
const MAX_WIDTH: i32 = 230;
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
const COPIED: Duration = Duration::from_millis(1500);
const EXPANDED_ABOVE: i32 = 900;
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
    painted: Cell<f64>,
    hovered: Cell<bool>,
    down: Cell<bool>,
    toggled: Cell<bool>,
    expanded: Cell<bool>,
    ticking: Cell<bool>,
    tip: Rc<Tooltip>,
    theme: SharedTheme,
}

pub struct Rail {
    pub root: FixedWidth,
    theme: SharedTheme,
    expanded: Cell<bool>,
    chosen: Cell<bool>,
    width: Cell<Tween>,
    ticking: Cell<bool>,
    toggle_icon: gtk4::Label,
    toggle_turn: Centred,
    fab: RippleButton,
    fab_icon: gtk4::Label,
    fab_text: gtk4::Label,
    copied: RefCell<Option<glib::SourceId>>,
    search: Rc<TextField>,
    scroller: gtk4::ScrolledWindow,
    list: gtk4::Box,
    results_scroller: gtk4::ScrolledWindow,
    results: gtk4::Box,
    hits: RefCell<Vec<Hit>>,
    highlight: Paint,
    highlight_y: Rc<anim::Motion>,
    tabs: Vec<Rc<Tab>>,
    current: Cell<usize>,
    revealing: Cell<bool>,
    selected: RefCell<Option<Selected>>,
    found: RefCell<Option<Found>>,
    _tips: Vec<Rc<Tooltip>>,
}

impl Rail {
    pub fn new(theme: &SharedTheme) -> Rc<Self> {
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, SPACING);

        let toggle = RippleButton::new(theme);
        toggle.set_radius(rounding::FULL as f64);
        toggle.set_size_request(TOGGLE_SIZE, TOGGLE_SIZE);
        toggle.set_halign(gtk4::Align::Start);
        toggle.set_margin_start(TOGGLE_MARGIN);
        let toggle_icon = text::symbol("menu_open", TOGGLE_ICON);
        text::set_color(&toggle_icon, "colOnLayer1");
        let toggle_turn = Centred::new(&toggle_icon);
        toggle.set_content(&toggle_turn, 0, 0);
        column.append(&toggle);

        let fab = RippleButton::new(theme);
        fab.set_look(FAB);
        fab.set_radius(FAB_RADIUS);
        fab.set_size_request(BASE, BASE);
        fab.set_overflow(gtk4::Overflow::Hidden);
        let fab_icon = text::symbol("edit", FAB_ICON);
        text::set_color(&fab_icon, "colOnPrimaryContainer");
        let fab_text = text::styled_sized("Config file", TAB_TEXT);
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

        let search = TextField::new(theme, Style::Outlined, "Search");
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
        crate::ui::widgets::flickable::follow_scroll_settings(&scroller);
        column.append(&scroller);

        let results = gtk4::Box::new(gtk4::Orientation::Vertical, RESULT_SPACING);
        let results_scroller = gtk4::ScrolledWindow::new();
        results_scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        results_scroller.set_vexpand(true);
        results_scroller.set_child(Some(&results));
        results_scroller.set_visible(false);
        crate::ui::widgets::flickable::follow_scroll_settings(&results_scroller);
        column.append(&results_scroller);

        let tabs: Vec<Rc<Tab>> = (0..PAGES.len()).map(|page| Tab::new(theme, page)).collect();
        for tab in &tabs {
            list.append(&tab.outer);
        }
        let widest = tabs.iter().map(|tab| tab.visual_width).max().unwrap_or(0);

        let root = FixedWidth::new(expanded_width(widest));
        root.set_child(&column);

        let fab_tip = Tooltip::new(&fab, theme, tooltip::Kind::Styled);
        fab_tip.set_text("Open the shell config file\nAlternatively right-click to copy path");
        tooltip::hover_delay(&fab, &fab_tip, 0);

        let rail = Rc::new(Rail {
            root,
            theme: theme.clone(),
            expanded: Cell::new(true),
            chosen: Cell::new(false),
            width: Cell::new(Tween::new(
                expanded_width(widest) as f64,
                WIDTH_MILLIS,
                EXPRESSIVE_EFFECTS,
            )),
            ticking: Cell::new(false),
            toggle_icon,
            toggle_turn,
            fab: fab.clone(),
            fab_icon,
            fab_text,
            copied: RefCell::new(None),
            search: search.clone(),
            scroller,
            list,
            results_scroller,
            results,
            hits: RefCell::new(Vec::new()),
            highlight: highlight.clone(),
            highlight_y,
            tabs,
            current: Cell::new(0),
            revealing: Cell::new(false),
            selected: RefCell::new(None),
            found: RefCell::new(None),
            _tips: vec![fab_tip],
        });

        toggle.connect_down({
            let rail = Rc::downgrade(&rail);
            move || {
                if let Some(rail) = rail.upgrade() {
                    rail.chosen.set(true);
                    rail.set_expanded(!rail.expanded.get());
                }
            }
        });
        fab.connect_down(|| {
            process::detach(&["xdg-open", &config::config_path().to_string_lossy()]);
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
                let first = rail.hits.borrow().first().cloned();
                if let Some(hit) = first {
                    rail.choose(&hit);
                }
            }
        });
        for tab in &rail.tabs {
            let press = gtk4::GestureClick::new();
            press.connect_pressed({
                let rail = Rc::downgrade(&rail);
                let page = tab.page;
                move |_, _, _, _| {
                    let Some(rail) = rail.upgrade() else {
                        return;
                    };
                    let selected = rail.selected.borrow().clone();
                    if let Some(selected) = selected {
                        selected(page);
                    }
                }
            });
            tab.button.add_controller(press);
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
        rail.set_current(0);
        rail
    }

    pub fn connect_selected(&self, action: impl Fn(usize) + 'static) {
        self.selected.replace(Some(Rc::new(action)));
    }

    pub fn connect_found(&self, action: impl Fn(&Hit) + 'static) {
        self.found.replace(Some(Rc::new(action)));
    }

    fn choose(&self, hit: &Hit) {
        let found = self.found.borrow().clone();
        if let Some(found) = found {
            found(hit);
        }
    }

    pub fn current(&self) -> usize {
        self.current.get()
    }

    pub fn set_current(&self, page: usize) {
        self.current.set(page);
        for tab in &self.tabs {
            tab.set_toggled(tab.page == page);
        }
        self.highlight.queue_draw();
        self.revealing.set(true);
        self.reveal_current();
    }

    pub fn follow_window_width(self: &Rc<Self>, width: i32) {
        if self.chosen.get() {
            return;
        }
        self.set_expanded(width > EXPANDED_ABOVE);
    }

    fn set_expanded(self: &Rc<Self>, expanded: bool) {
        if self.expanded.replace(expanded) == expanded {
            return;
        }
        self.toggle_icon
            .set_text(if expanded { "menu_open" } else { "menu" });
        self.toggle_turn
            .rotate_to(if expanded { 0.0 } else { -180.0 });
        self.search.root.set_visible(expanded);
        self.fab_text.set_visible(expanded);
        if !expanded {
            self.search.set_text("");
        }
        self.fab.set_halign(if expanded {
            gtk4::Align::Fill
        } else {
            gtk4::Align::Start
        });
        for tab in &self.tabs {
            tab.set_expanded(expanded);
        }
        let widest = self
            .tabs
            .iter()
            .map(|tab| tab.visual_width)
            .max()
            .unwrap_or(0);
        let target = if expanded {
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
        if self.ticking.replace(true) {
            return;
        }
        let rail = self.clone();
        root.add_tick_callback(move |root, clock| {
            let tween = rail.width.get();
            let now = clock.frame_time();
            root.set_width(tween.value(now).round() as i32);
            rail.highlight.queue_draw();
            if tween.running(now) {
                return glib::ControlFlow::Continue;
            }
            rail.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn filter(self: &Rc<Self>, query: &str) {
        let searching = !query.trim().is_empty();
        self.scroller.set_visible(!searching);
        self.results_scroller.set_visible(searching);
        while let Some(child) = self.results.first_child() {
            self.results.remove(&child);
        }
        let hits = index::search(query);
        if searching && hits.is_empty() {
            let nothing = text::styled_sized("No settings found", TAB_TEXT);
            text::set_color(&nothing, "colSubtext");
            nothing.set_margin_top(RESULT_PADDING);
            self.results.append(&nothing);
        }
        for hit in hits.iter().take(RESULT_LIMIT) {
            self.results.append(&self.result_row(hit));
        }
        self.results_scroller.vadjustment().set_value(0.0);
        self.hits.replace(hits);
        if !searching {
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
        lines.append(&Centred::filling_width(&title));
        if !hit.trail.is_empty() {
            let trail = text::styled_sized(&hit.trail, pixel_size::SMALLER);
            text::set_color(&trail, "colSubtext");
            trail.set_xalign(0.0);
            trail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centred::filling_width(&trail));
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
        self.fab_text.set_text("Path copied");
        if let Some(pending) = self.copied.take() {
            pending.remove();
        }
        let icon = self.fab_icon.clone();
        let label = self.fab_text.clone();
        let pending = glib::timeout_add_local_once(COPIED, move || {
            icon.set_text("edit");
            label.set_text("Config file");
        });
        self.copied.replace(Some(pending));
    }

    fn reveal_current(&self) {
        if !self.revealing.get() {
            return;
        }
        let tab = &self.tabs[self.current.get()];
        let adjustment = self.scroller.vadjustment();
        let Some(bounds) = tab.button.compute_bounds(&self.list) else {
            return;
        };
        if adjustment.page_size() <= 0.0 {
            return;
        }
        self.revealing.set(false);
        let group = if tab.separator.is_visible() {
            GROUP_SPACING as f64
        } else {
            0.0
        };
        let top = bounds.y() as f64 - group;
        let bottom = (bounds.y() + bounds.height()) as f64;
        if top < adjustment.value() {
            adjustment.set_value(top);
        } else if bottom > adjustment.value() + adjustment.page_size() {
            adjustment.set_value(bottom - adjustment.page_size());
        }
    }

    fn draw_highlight(&self, snapshot: &gtk4::Snapshot) {
        let tab = &self.tabs[self.current.get()];
        let Some(bounds) = tab.button.compute_bounds(&self.list) else {
            return;
        };
        let expanded = self.expanded.get();
        let offset = if expanded {
            0.0
        } else {
            ((BASE - HIGHLIGHT_HEIGHT) / 2) as f64
        };
        let target = bounds.y() as f64 + offset;
        if (self.highlight_y.get() - target).abs() > f64::EPSILON && !self.highlight_y.running() {
            let motion = self.highlight_y.clone();
            glib::idle_add_local_once(move || motion.to(target));
        }
        let height = if expanded { BASE } else { HIGHLIGHT_HEIGHT } as f32;
        let width = if expanded {
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
    widest.clamp(MIN_WIDTH, MAX_WIDTH)
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
        let icon_box = Centred::integral(&icon);
        icon_box.set_size_request(BASE, HIGHLIGHT_HEIGHT);
        icon_box.set_valign(gtk4::Align::Center);
        let label = text::styled_sized(info.name, TAB_TEXT);
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
        tip.set_text(info.name);

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
            painted: Cell::new(f64::NAN),
            hovered: Cell::new(false),
            down: Cell::new(false),
            toggled: Cell::new(false),
            expanded: Cell::new(true),
            ticking: Cell::new(false),
            tip,
            theme: theme.clone(),
        });

        let motion = gtk4::EventControllerMotion::new();
        motion.connect_enter({
            let tab = Rc::downgrade(&tab);
            move |_, _, _| {
                if let Some(tab) = tab.upgrade() {
                    tab.hovered.set(true);
                    tab.tip.show(!tab.expanded.get());
                    tab.restyle(true);
                }
            }
        });
        motion.connect_leave({
            let tab = Rc::downgrade(&tab);
            move |_| {
                if let Some(tab) = tab.upgrade() {
                    tab.hovered.set(false);
                    tab.tip.show(false);
                    tab.restyle(true);
                }
            }
        });
        button.add_controller(motion);
        let click = gtk4::GestureClick::new();
        click.connect_pressed({
            let tab = Rc::downgrade(&tab);
            move |_, _, _, _| {
                if let Some(tab) = tab.upgrade() {
                    tab.down.set(true);
                    tab.restyle(true);
                }
            }
        });
        click.connect_released({
            let tab = Rc::downgrade(&tab);
            move |_, _, _, _| {
                if let Some(tab) = tab.upgrade() {
                    tab.down.set(false);
                    tab.restyle(true);
                }
            }
        });
        click.connect_cancel({
            let tab = Rc::downgrade(&tab);
            move |_, _| {
                if let Some(tab) = tab.upgrade() {
                    tab.down.set(false);
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

    fn set_toggled(self: &Rc<Self>, toggled: bool) {
        if self.toggled.replace(toggled) == toggled {
            return;
        }
        self.restyle(true);
    }

    fn set_expanded(&self, expanded: bool) {
        self.expanded.set(expanded);
        self.label.set_visible(expanded);
        if expanded {
            self.tip.show(false);
        }
        self.background.queue_draw();
    }

    fn restyle(self: &Rc<Self>, animate: bool) {
        let toggled = self.toggled.get();
        self.icon.add_css_class("color-fade");
        text::set_color(
            &self.icon,
            if toggled {
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
        let animate = animate && self.background.is_mapped();
        self.fade.retarget(target, now, animate);
        let mut fill = self.fill.get();
        if animate {
            fill.retarget(if toggled { 1.0 } else { 0.0 }, now);
        } else {
            fill.jump(if toggled { 1.0 } else { 0.0 });
        }
        self.fill.set(fill);
        self.paint_fill(now);
        self.background.queue_draw();
        if self.ticking.replace(true) {
            return;
        }
        let tab = self.clone();
        self.background.add_tick_callback(move |background, clock| {
            let now = clock.frame_time();
            background.queue_draw();
            tab.paint_fill(now);
            if tab.fade.running(now) || tab.fill.get().running(now) {
                return glib::ControlFlow::Continue;
            }
            tab.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn paint_fill(&self, now: i64) {
        let fill = (self.fill.get().value(now) * 10.0).round() / 10.0;
        if self.painted.replace(fill) == fill {
            return;
        }
        text::set_symbol_font_weighted(&self.icon, TAB_ICON, fill, 400.0);
    }

    fn target(&self, theme: &Theme) -> RGBA {
        if self.toggled.get() {
            return transparentize(theme.colors.col_secondary_container, 1.0);
        }
        if self.down.get() {
            return theme.colors.col_layer1_active;
        }
        if self.hovered.get() {
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
        let Some(colour) = self.fade.value(now) else {
            return;
        };
        let rect = if self.expanded.get() {
            graphene::Rect::new(0.0, 0.0, width.max(self.visual_width as f32), height)
        } else {
            let top = ((height - HIGHLIGHT_HEIGHT as f32) / 2.0).round();
            graphene::Rect::new(0.0, top, BASE as f32, HIGHLIGHT_HEIGHT as f32)
        };
        let radius = rect.width().min(rect.height()) / 2.0;
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(rect, radius));
        snapshot.append_color(&colour, &rect);
        snapshot.pop();
    }
}
