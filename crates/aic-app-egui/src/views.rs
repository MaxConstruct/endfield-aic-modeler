//! The canvas's views (ADR 0008): the open graph shown in any number of tabs, split side by side or
//! one above another, and on the desktop in windows of their own.
//!
//! Each view has its own level and camera; the graph, its solution, undo and the selection are
//! shared. The focused view (the one last clicked) lives in the editor: its [`Canvas`] is
//! `Editor::canvas` and its level is the [`Document`](crate::document::Document)'s. Every other
//! view waits in a [`Parked`] and is swapped in to be drawn, so the document's edits always act on
//! the view being drawn.
//!
//! Tabs are laid out by egui_dock. A tab it would float in an egui window (dragged out of the dock,
//! or Move to new window) becomes a native window instead: an egui immediate viewport showing only
//! the canvas. The web build has no native windows, so there a tab can't leave the dock.

use std::collections::HashMap;

use aic_core::{CanvasView, WorkspaceId};
use eframe::egui::{self, Id, Rect, Ui, WidgetText};
use egui_dock::{DockArea, DockState, Node, NodePath, Split, Surface, SurfaceIndex, TabViewer};
use serde::{Deserialize, Serialize};

use crate::canvas::Canvas;
use crate::document::DocView;
use crate::editor::Editor;
use crate::i18n::Strings;
use crate::theme;
use aic_editor::names;

/// The storage key for the layout of the views.
pub const STORAGE_KEY: &str = "views";
/// Whether views can open in windows of their own: not in the browser.
pub const WINDOWS: bool = cfg!(not(target_arch = "wasm32"));
/// A new window's size, in points.
const WINDOW_SIZE: [f32; 2] = [900.0, 640.0];

/// One view's number, unique while the app runs and kept with the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ViewKey(u64);

/// A view's own state while another view is focused.
struct Parked {
    canvas: Canvas,
    level: DocView,
}

/// A native window of views.
pub(crate) struct ViewWindow {
    /// Numbers the window's viewport; unique while the app runs.
    id: u64,
    dock: DockState<ViewKey>,
    /// Where the window was last, in points on the desktop.
    rect: Option<Rect>,
    /// Asked to open here: the first time, or after a restart.
    open_at: Option<Rect>,
}

/// Every view, how they are laid out, and which one has focus.
pub struct Views {
    main: DockState<ViewKey>,
    windows: Vec<ViewWindow>,
    parked: HashMap<ViewKey, Parked>,
    focused: ViewKey,
    next: u64,
    /// What the tab menus and shortcuts asked for this frame, done once the dock is drawn.
    actions: Vec<ViewAction>,
}

/// Something done to a view from its tab or with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewAction {
    SplitRight(ViewKey),
    SplitDown(ViewKey),
    Duplicate(ViewKey),
    MoveToWindow(ViewKey),
    Close(ViewKey),
    Focus(ViewKey),
    /// Opens a workspace in a new tab beside the view.
    OpenBeside(ViewKey, WorkspaceId),
}

impl Default for Views {
    fn default() -> Self {
        Self {
            main: DockState::new(vec![ViewKey(0)]),
            windows: Vec::new(),
            parked: HashMap::new(),
            focused: ViewKey(0),
            next: 1,
            actions: Vec::new(),
        }
    }
}

impl Views {
    /// The view with focus.
    pub fn focused(&self) -> ViewKey {
        self.focused
    }

    /// How many views there are.
    pub fn len(&self) -> usize {
        self.parked.len() + 1
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// Every view, the main window's first, in tab order.
    pub fn keys(&self) -> Vec<ViewKey> {
        self.docks()
            .flat_map(|dock| dock.iter_all_tabs().map(|(_, key)| *key))
            .collect()
    }

    /// How many native windows of views are open.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Whether `key` is in a window of its own rather than the main window.
    pub fn is_in_window(&self, key: ViewKey) -> bool {
        self.windows.iter().any(|w| w.dock.find_tab(&key).is_some())
    }

    /// Asks for `action` once the views are next drawn.
    pub fn request(&mut self, action: ViewAction) {
        self.actions.push(action);
    }

    fn docks(&self) -> impl Iterator<Item = &DockState<ViewKey>> {
        std::iter::once(&self.main).chain(self.windows.iter().map(|w| &w.dock))
    }

    fn docks_mut(&mut self) -> impl Iterator<Item = &mut DockState<ViewKey>> {
        std::iter::once(&mut self.main).chain(self.windows.iter_mut().map(|w| &mut w.dock))
    }

    fn new_key(&mut self) -> ViewKey {
        let key = ViewKey(self.next);
        self.next += 1;
        key
    }
}

/// The layout as kept in eframe storage: the views, their levels and cameras, and the windows.
/// A level is a workspace's place among the graph's workspaces, which is what a file keeps, so it
/// still names the same workspace once the file is opened again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    main: DockState<ViewKey>,
    windows: Vec<(DockState<ViewKey>, Option<[f32; 4]>)>,
    views: Vec<(ViewKey, Option<usize>, Option<CanvasView>)>,
    focused: ViewKey,
}

impl Editor {
    /// The level a view shows.
    pub fn view_level(&self, key: ViewKey) -> Option<WorkspaceId> {
        if key == self.views.focused {
            self.view.level()
        } else {
            self.views.parked.get(&key).and_then(|p| p.level.level())
        }
    }

    /// Where a view is looking.
    fn view_camera(&self, key: ViewKey) -> Option<CanvasView> {
        if key == self.views.focused {
            self.canvas.view()
        } else {
            self.views.parked.get(&key).and_then(|p| p.canvas.view())
        }
    }

    pub fn views(&self) -> &Views {
        &self.views
    }

    pub fn views_mut(&mut self) -> &mut Views {
        &mut self.views
    }

    /// Exchanges the focused view's state in the editor with `key`'s. Doing it twice undoes it.
    fn swap(&mut self, key: ViewKey) {
        let parked = self
            .views
            .parked
            .get_mut(&key)
            .expect("every view but the focused one is parked");
        std::mem::swap(&mut self.canvas, &mut parked.canvas);
        self.view.swap(
            self.session.document(),
            self.session.catalog(),
            &mut parked.level,
        );
    }

    /// Gives `key` the focus: its canvas and level move into the editor.
    pub fn focus_view(&mut self, key: ViewKey) {
        let old = self.views.focused;
        if key == old || !self.views.parked.contains_key(&key) {
            return;
        }
        self.swap(key);
        let parked = self.views.parked.remove(&key).expect("checked above");
        self.views.parked.insert(old, parked);
        self.views.focused = key;
        self.picker = None;
        self.port_menu = None;
    }

    /// A new view of the same level as `from`, looking at the same place.
    fn new_view(&mut self, from: ViewKey) -> ViewKey {
        let key = self.views.new_key();
        let level = self.view_level(from);
        let mut canvas = Canvas::new(key.0);
        if let Some(camera) = self.view_camera(from) {
            canvas.show_view_at(level, camera);
        }
        self.views.parked.insert(
            key,
            Parked {
                canvas,
                level: DocView::new(level),
            },
        );
        key
    }

    /// A new view of `level`, beside `from`.
    fn new_view_of(&mut self, level: Option<WorkspaceId>) -> ViewKey {
        let key = self.views.new_key();
        self.views.parked.insert(
            key,
            Parked {
                canvas: Canvas::new(key.0),
                level: DocView::new(level),
            },
        );
        key
    }

    /// Opening another graph shows its top in every view.
    pub(crate) fn reset_views(&mut self) {
        for parked in self.views.parked.values_mut() {
            parked.level = DocView::new(None);
        }
    }

    /// The tab's name: the region's at the top, the workspace's inside one, numbered when another
    /// view shows the same level.
    pub fn view_title(&self, key: ViewKey, strings: &Strings) -> String {
        let level = self.view_level(key);
        let title = match level {
            None => names::region(
                self.session.catalog(),
                self.session.document().graph().region().as_str(),
            )
            .to_owned(),
            Some(id) => self
                .session
                .document()
                .graph()
                .workspace_info(id)
                .map(|w| w.name.clone())
                .unwrap_or_default(),
        };
        // Every view, whether or not its dock is out being drawn.
        let mut same: Vec<ViewKey> = self
            .views
            .parked
            .keys()
            .copied()
            .chain([self.views.focused])
            .filter(|other| self.view_level(*other) == level)
            .collect();
        same.sort();
        match same.iter().position(|other| *other == key) {
            Some(index) if index > 0 => {
                let number = (index + 1).to_string();
                strings.format("view-numbered", &[("title", &title), ("number", &number)])
            }
            _ => title,
        }
    }

    /// The canvas area of the main window: the one view on its own, or the dock once there are
    /// more.
    pub(crate) fn views_ui(&mut self, ui: &mut Ui, strings: &Strings) {
        if self.views.len() == 1 && self.views.windows.is_empty() {
            // One view needs no tab: the canvas looks as it always did.
            let key = self.views.focused;
            self.view_ui(ui, key, strings);
        } else {
            let mut dock = std::mem::replace(&mut self.views.main, DockState::new(Vec::new()));
            self.dock_ui(ui, &mut dock, strings, Id::new("view-dock"));
            self.views.main = dock;
        }
    }

    /// The native windows of views. Called from the main window, after its own views.
    pub(crate) fn view_windows(&mut self, ctx: &egui::Context, strings: &Strings) {
        if !WINDOWS {
            return;
        }
        let mut windows = std::mem::take(&mut self.views.windows);
        let mut closed = Vec::new();
        for (index, window) in windows.iter_mut().enumerate() {
            let viewport = egui::ViewportId::from_hash_of(("view-window", window.id));
            let title = window
                .dock
                .iter_all_tabs()
                .next()
                .map(|(_, key)| self.view_title(*key, strings))
                .unwrap_or_default();
            let mut builder = egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(WINDOW_SIZE);
            if let Some(rect) = window.open_at.take() {
                builder = builder.with_position(rect.min).with_inner_size(rect.size());
            }
            ctx.show_viewport_immediate(viewport, builder, |ui, _class| {
                if ui.input(|i| i.viewport().close_requested()) {
                    closed.push(index);
                }
                window.rect = ui.input(|i| i.viewport().outer_rect);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        self.dock_ui(
                            ui,
                            &mut window.dock,
                            strings,
                            Id::new(("view-dock", window.id)),
                        );
                    });
                if window.dock.find_tab(&self.views.focused).is_some() {
                    self.keyboard(ui.ctx());
                    self.overlays_ui(ui.ctx(), strings);
                }
            });
        }
        for index in closed.into_iter().rev() {
            let window = windows.remove(index);
            for (_, key) in window.dock.iter_all_tabs() {
                self.views.actions.push(ViewAction::Close(*key));
            }
        }
        // Windows made while these were drawn come after them.
        windows.append(&mut self.views.windows);
        self.views.windows = windows;
    }

    /// Shows `dock` in `ui`, then turns tabs egui_dock floated into native windows.
    fn dock_ui(&mut self, ui: &mut Ui, dock: &mut DockState<ViewKey>, strings: &Strings, id: Id) {
        let translations = egui_dock::Translations {
            tab_context_menu: egui_dock::TabContextMenuTranslations {
                close_button: strings.get("view-close"),
                eject_button: strings.get("view-move-to-window"),
                ..egui_dock::TabContextMenuTranslations::english()
            },
            ..egui_dock::Translations::english()
        };
        dock.translations = translations;
        let total = self.views.len();
        let mut tabs = Tabs {
            editor: self,
            strings,
            total,
        };
        DockArea::new(dock)
            .id(id)
            .style(dock_style(ui.style()))
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .show_inside(ui, &mut tabs);
        self.float_to_windows(dock, ui.ctx());
    }

    /// Moves every tab egui_dock put in a floating egui window into a native window.
    fn float_to_windows(&mut self, dock: &mut DockState<ViewKey>, ctx: &egui::Context) {
        let floating: Vec<SurfaceIndex> = dock
            .iter_surfaces_indexed()
            .filter(|(_, surface)| matches!(surface, Surface::Window(..)))
            .map(|(index, _)| index)
            .collect();
        for index in floating {
            if let Some(Surface::Window(tree, _)) = dock.remove_surface(index) {
                let keys: Vec<ViewKey> = tree.tabs().copied().collect();
                self.open_window(keys, ctx);
            }
        }
    }

    /// Opens a native window of `keys`, beside the main window.
    fn open_window(&mut self, keys: Vec<ViewKey>, ctx: &egui::Context) {
        if keys.is_empty() {
            return;
        }
        let id = self
            .views
            .windows
            .iter()
            .map(|w| w.id + 1)
            .max()
            .unwrap_or(0)
            + self.views.next;
        let open_at = ctx.input(|i| i.viewport().outer_rect).map(|main| {
            let offset = 32.0 * (self.views.windows.len() + 1) as f32;
            Rect::from_min_size(main.min + egui::vec2(offset, offset), WINDOW_SIZE.into())
        });
        self.views.windows.push(ViewWindow {
            id,
            dock: DockState::new(keys),
            rect: None,
            open_at,
        });
    }

    /// Draws one view: swapped in, drawn, its edits applied, and swapped out again unless the
    /// player clicked in it, which gives it the focus.
    fn view_ui(&mut self, ui: &mut Ui, key: ViewKey, strings: &Strings) {
        let focused = key == self.views.focused;
        if !focused {
            self.swap(key);
        }
        // Edits made earlier in the frame (menus, toolbar, browser, inspector) show now.
        self.sync_view();
        let events = self.canvas.show(ui, &mut self.view, &self.session, strings);
        let rect = self.canvas.rect;
        let pressed = ui.input(|i| {
            i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| rect.contains(p))
        });
        let open_beside = ui.input(|i| i.modifiers.command || i.pointer.middle_down());
        for event in events {
            match event {
                crate::canvas::CanvasEvent::ShowLevel(Some(workspace)) if open_beside => {
                    self.views
                        .actions
                        .push(ViewAction::OpenBeside(key, workspace));
                }
                event => self.apply(event),
            }
        }
        self.sync_view();
        self.view.sync_from_view(&mut self.session);
        if !focused {
            self.swap(key);
            if pressed {
                self.focus_view(key);
            }
        }
    }

    /// Does what the tab menus and view shortcuts asked for, and drops views whose tabs were
    /// closed.
    pub(crate) fn finish_views(&mut self, ctx: &egui::Context) {
        let actions = std::mem::take(&mut self.views.actions);
        let before = self.views.len();
        if !actions.is_empty() {
            // The layout changed after it was drawn.
            ctx.request_repaint();
        }
        for action in actions {
            self.do_view_action(action, ctx);
        }
        self.drop_closed_views();
        if self.views.len() != before {
            ctx.request_repaint();
        }
    }

    fn do_view_action(&mut self, action: ViewAction, ctx: &egui::Context) {
        match action {
            ViewAction::SplitRight(key) => self.split(key, Split::Right),
            ViewAction::SplitDown(key) => self.split(key, Split::Below),
            ViewAction::Duplicate(key) => {
                let new = self.new_view(key);
                self.add_beside(key, new, None);
            }
            ViewAction::MoveToWindow(key) => {
                if !WINDOWS {
                    return;
                }
                if let Some((dock, path)) = self.find(key) {
                    let dock = self.dock_mut(dock);
                    if dock.remove_tab(path).is_some() {
                        self.open_window(vec![key], ctx);
                    }
                }
            }
            ViewAction::Close(key) => {
                if self.views.len() > 1
                    && let Some((dock, path)) = self.find(key)
                {
                    self.dock_mut(dock).remove_tab(path);
                }
            }
            ViewAction::Focus(key) => self.focus_view(key),
            ViewAction::OpenBeside(key, workspace) => {
                let new = self.new_view_of(Some(workspace));
                self.add_beside(key, new, None);
                self.focus_view(new);
            }
        }
    }

    /// Splits `key`'s place in two, with a new view of the same level on the `split` side.
    fn split(&mut self, key: ViewKey, split: Split) {
        let new = self.new_view(key);
        self.add_beside(key, new, Some(split));
        self.focus_view(new);
    }

    /// Puts `new` in `key`'s tab group, or in a new group on the `split` side of it.
    fn add_beside(&mut self, key: ViewKey, new: ViewKey, split: Option<Split>) {
        let Some((dock, path)) = self.find(key) else {
            self.views.main.push_to_first_leaf(new);
            return;
        };
        let dock = self.dock_mut(dock);
        let node = NodePath {
            surface: path.surface,
            node: path.node,
        };
        match split {
            Some(split) => {
                dock.split(node, split, 0.5, Node::leaf(new));
            }
            None => {
                if let Ok(leaf) = dock.leaf_mut(node) {
                    leaf.append_tab(new);
                }
            }
        }
    }

    /// Which dock holds `key` (0 is the main window's, then the windows'), and where in it.
    fn find(&self, key: ViewKey) -> Option<(usize, egui_dock::TabPath)> {
        self.views
            .docks()
            .enumerate()
            .find_map(|(index, dock)| dock.find_tab(&key).map(|path| (index, path)))
    }

    fn dock_mut(&mut self, index: usize) -> &mut DockState<ViewKey> {
        self.views.docks_mut().nth(index).expect("found above")
    }

    /// Forgets views no dock shows any more, and windows left empty. The focus moves to another
    /// view first if its own is gone; there is always one view.
    fn drop_closed_views(&mut self) {
        self.views
            .windows
            .retain(|w| w.dock.iter_all_tabs().next().is_some());
        let shown = self.views.keys();
        if shown.is_empty() {
            // Every tab went (a closed window held the last ones): bring the focused one back.
            self.views.main = DockState::new(vec![self.views.focused]);
        } else if !shown.contains(&self.views.focused) {
            self.focus_view(shown[0]);
        }
        let shown = self.views.keys();
        self.views.parked.retain(|key, _| shown.contains(key));
    }

    /// The layout to keep: see [`Layout`].
    pub fn layout(&self) -> Layout {
        let workspaces: Vec<WorkspaceId> = self
            .session
            .document()
            .graph()
            .workspaces()
            .keys()
            .collect();
        let place = |level: Option<WorkspaceId>| {
            level.and_then(|id| workspaces.iter().position(|w| *w == id))
        };
        let mut views: Vec<_> = self
            .views
            .keys()
            .into_iter()
            .map(|key| (key, place(self.view_level(key)), self.view_camera(key)))
            .collect();
        views.sort_by_key(|(key, ..)| *key);
        Layout {
            main: self.views.main.clone(),
            windows: self
                .views
                .windows
                .iter()
                .map(|w| {
                    let rect = w.rect.map(|r| [r.min.x, r.min.y, r.width(), r.height()]);
                    (w.dock.clone(), rect)
                })
                .collect(),
            views,
            focused: self.views.focused,
        }
    }

    /// Lays the views out as `layout` keeps them, on the open graph. A layout that doesn't hold
    /// together (a view in two places, or none shown) is left alone.
    pub fn restore_layout(&mut self, layout: Layout) {
        let Layout {
            main,
            windows,
            views,
            focused,
        } = layout;
        let windows: Vec<_> = if WINDOWS { windows } else { Vec::new() };
        let mut shown: Vec<ViewKey> = main.iter_all_tabs().map(|(_, key)| *key).collect();
        for (dock, _) in &windows {
            shown.extend(dock.iter_all_tabs().map(|(_, key)| *key));
        }
        let mut unique = shown.clone();
        unique.sort();
        unique.dedup();
        let known: HashMap<ViewKey, (Option<usize>, Option<CanvasView>)> = views
            .into_iter()
            .map(|(key, level, camera)| (key, (level, camera)))
            .collect();
        if shown.is_empty()
            || unique.len() != shown.len()
            || !shown.contains(&focused)
            || shown.iter().any(|key| !known.contains_key(key))
        {
            return;
        }
        let workspaces: Vec<WorkspaceId> = self
            .session
            .document()
            .graph()
            .workspaces()
            .keys()
            .collect();
        let level_at = |place: Option<usize>| place.and_then(|i| workspaces.get(i).copied());
        let canvas_for = |key: ViewKey, level, camera: Option<CanvasView>| {
            let mut canvas = Canvas::new(key.0);
            if let Some(camera) = camera {
                canvas.show_view_at(level, camera);
            }
            canvas
        };
        let mut parked = HashMap::new();
        for key in &shown {
            let (place, camera) = known[key];
            let level = level_at(place);
            if *key == focused {
                self.canvas = canvas_for(*key, level, camera);
                self.view
                    .set_level(self.session.document(), self.session.catalog(), level);
            } else {
                parked.insert(
                    *key,
                    Parked {
                        canvas: canvas_for(*key, level, camera),
                        level: DocView::new(level),
                    },
                );
            }
        }
        let next = shown.iter().map(|k| k.0 + 1).max().unwrap_or(0);
        self.views = Views {
            main,
            windows: windows
                .into_iter()
                .enumerate()
                .map(|(index, (dock, rect))| {
                    let open_at = rect.map(|[x, y, w, h]| {
                        Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h))
                    });
                    ViewWindow {
                        id: next + index as u64,
                        dock,
                        rect: open_at,
                        open_at,
                    }
                })
                .collect(),
            parked,
            focused,
            next: next + 1000,
            actions: Vec::new(),
        };
    }
}

/// egui_dock's look in the app's theme: tabs on the panel colour, a hairline under the bar, and a
/// translucent accent fill where a dragged tab would land.
fn dock_style(style: &egui::Style) -> egui_dock::Style {
    use theme::color;
    let mut dock = egui_dock::Style::from_egui(style);
    dock.dock_area_padding = None;
    dock.main_surface_border_stroke = egui::Stroke::NONE;
    dock.tab_bar.bg_fill = color::PANEL;
    dock.tab_bar.hline_color = color::DIVIDER;
    dock.tab_bar.corner_radius = egui::CornerRadius::ZERO;
    dock.separator.width = 1.0;
    dock.separator.color_idle = color::DIVIDER;
    dock.separator.color_hovered = color::BORDER_STRONG;
    dock.separator.color_dragged = color::ACCENT;
    for tab in [
        &mut dock.tab.active,
        &mut dock.tab.focused,
        &mut dock.tab.active_with_kb_focus,
        &mut dock.tab.focused_with_kb_focus,
    ] {
        tab.bg_fill = color::CANVAS;
        tab.text_color = color::TEXT_STRONG;
        tab.outline_color = color::DIVIDER;
        tab.corner_radius = egui::CornerRadius::ZERO;
    }
    for tab in [
        &mut dock.tab.inactive,
        &mut dock.tab.hovered,
        &mut dock.tab.inactive_with_kb_focus,
    ] {
        tab.bg_fill = color::PANEL;
        tab.text_color = color::TEXT_MUTED;
        tab.outline_color = egui::Color32::TRANSPARENT;
        tab.corner_radius = egui::CornerRadius::ZERO;
    }
    dock.tab.hovered.text_color = color::TEXT;
    dock.tab.tab_body.inner_margin = egui::Margin::ZERO;
    dock.tab.tab_body.stroke = egui::Stroke::NONE;
    dock.tab.tab_body.bg_fill = color::CANVAS;
    dock.tab.hline_below_active_tab_name = false;
    dock.overlay.overlay_type = egui_dock::OverlayType::HighlightedAreas;
    dock.overlay.selection_color = color::ACCENT.gamma_multiply(0.25);
    dock.overlay.selection_stroke_width = 1.0;
    dock.buttons.close_tab_color = color::TEXT_FAINT;
    dock.buttons.close_tab_active_color = color::TEXT_STRONG;
    dock.buttons.close_tab_bg_fill = color::CONTROL_HOVER;
    dock
}

/// egui_dock's view of the editor while it draws the tabs.
struct Tabs<'a> {
    editor: &'a mut Editor,
    strings: &'a Strings,
    /// How many views there are; the last one can't be closed.
    total: usize,
}

impl TabViewer for Tabs<'_> {
    type Tab = ViewKey;

    fn id(&mut self, tab: &mut ViewKey) -> Id {
        Id::new(("view", tab.0))
    }

    fn title(&mut self, tab: &mut ViewKey) -> WidgetText {
        self.editor.view_title(*tab, self.strings).into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut ViewKey) {
        self.editor.view_ui(ui, *tab, self.strings);
    }

    fn context_menu(&mut self, ui: &mut Ui, tab: &mut ViewKey, _path: NodePath) {
        let strings = self.strings;
        let items = [
            ("view-split-right", ViewAction::SplitRight(*tab)),
            ("view-split-down", ViewAction::SplitDown(*tab)),
            ("view-duplicate", ViewAction::Duplicate(*tab)),
        ];
        for (key, action) in items {
            if ui.button(strings.get(key)).clicked() {
                self.editor.views.actions.push(action);
                ui.close();
            }
        }
        ui.separator();
    }

    fn on_tab_button(&mut self, tab: &mut ViewKey, response: &egui::Response) {
        if response.clicked() || response.drag_started() {
            self.editor.views.actions.push(ViewAction::Focus(*tab));
        }
        // The focused view's tab is underlined in the accent colour.
        if *tab == self.editor.views.focused {
            let px = response.ctx.pixels_per_point().recip();
            let rect = response.rect;
            response.ctx.layer_painter(response.layer_id).hline(
                rect.x_range(),
                rect.bottom() - px,
                egui::Stroke::new(px, theme::color::ACCENT),
            );
        }
    }

    fn is_closeable(&self, _tab: &ViewKey) -> bool {
        self.total > 1
    }

    fn allowed_in_windows(&self, _tab: &mut ViewKey) -> bool {
        WINDOWS
    }

    fn clear_background(&self, _tab: &ViewKey) -> bool {
        false
    }

    fn scroll_bars(&self, _tab: &ViewKey) -> [bool; 2] {
        [false, false]
    }
}
