//! Everything on screen once game data has loaded: the menu bar, the recipe browser, the canvas, the
//! inspector, the node picker, the dialogs and the settings window.

use std::path::PathBuf;
use std::time::Duration;

use aic_core::{CanvasView, Member, NodeId, Side, WorkspaceId};
use aic_data::{Catalog, RegionId};
use eframe::egui::{self, Pos2, RichText, Ui};

use crate::canvas::{self, Canvas, CanvasEvent, Selection};
use crate::data::{DataChoice, DataFolder};
use crate::document::{DocView, DocumentMut, DocumentRef, to_pos};
use crate::edit::Shortcut;
use crate::files::{Download, Pending, Upload};
use crate::format;
use crate::i18n::Strings;
use crate::names;
use crate::picker::{self, Choice, Origin, Picker};
use crate::port_menu::{self, PortMenu};
use crate::settings::{Availability, SettingsWindow};
use crate::views::Views;
use crate::widgets::colors;
use aic_editor::Session;
use aic_editor::picker::recipe_node;

/// A solve slower than this is reported in the status bar.
const SLOW_SOLVE: Duration = Duration::from_millis(16);
/// The storage key for whether the status bar shows the frame rate and solve time.
pub const PERFORMANCE_KEY: &str = "show-performance";
/// The frame rate is the mean over the frames of the last second.
const FRAME_RATE_WINDOW: f32 = 1.0;
/// The status bar's zoom slider.
const ZOOM_SLIDER_WIDTH: f32 = 96.0;
/// Space between a port's pin and the menu it opens.
const PORT_MENU_GAP: f32 = 12.0;

pub struct Editor {
    /// The catalog, the document, the selection, the clipboard and every edit that isn't drawing.
    pub(crate) session: Session,
    /// The egui-snarl view of the focused view's level of the document.
    pub(crate) view: DocView,
    /// The focused view's canvas; the other views' wait in `views`.
    pub(crate) canvas: Canvas,
    pub(crate) views: Views,
    pub(crate) picker: Option<Picker>,
    pub(crate) settings: SettingsWindow,
    pub(crate) browser: crate::browser::Browser,
    /// What the player is typing in the inspector's limit field.
    pub(crate) limit_edit: String,
    pub(crate) port_menu: Option<PortMenu>,
    pub(crate) show_browser: bool,
    pub(crate) show_inspector: bool,
    /// Files for the browser to download.
    pub(crate) downloads: Vec<Download>,
    /// The name typed into the browser's Save As prompt, while it is open.
    pub(crate) save_as_prompt: Option<String>,
    /// The file the player is picking in the browser, once read.
    pub(crate) upload: Option<std::sync::mpsc::Receiver<Upload>>,
    /// Decoded icons belong to the old data after a switch.
    pub(crate) icons_stale: bool,
    /// The shortcut last pressed, and when: its toolbar button lights up for a moment.
    pub(crate) flash: Option<(Shortcut, web_time::Instant)>,
    /// Where to save the canvas once the window is next drawn.
    pub(crate) export_to: Option<PathBuf>,
    /// Whether the status bar shows the frame rate and the last solve's time.
    pub(crate) show_performance: bool,
    /// When the last frames were drawn, for the frame rate.
    frames: egui::util::History<()>,
    pub(crate) last_title: String,
}

impl Editor {
    pub fn new(catalog: Catalog, availability: Availability) -> Self {
        Self {
            session: Session::new(catalog, availability),
            view: DocView::new(None),
            canvas: Canvas::default(),
            views: Views::default(),
            picker: None,
            settings: SettingsWindow::default(),
            browser: crate::browser::Browser::default(),
            limit_edit: String::new(),
            port_menu: None,
            show_browser: true,
            show_inspector: false,
            downloads: Vec::new(),
            save_as_prompt: None,
            upload: None,
            icons_stale: false,
            export_to: None,
            flash: None,
            show_performance: true,
            frames: egui::util::History::new(2..1000, FRAME_RATE_WINDOW),
            last_title: String::new(),
        }
    }

    /// Starts an empty graph in `region`, dropping the open one.
    pub fn new_graph(&mut self, region: RegionId) {
        self.session.new_graph(region);
        self.after_new_document();
    }

    /// Another document is open: every view shows its top, and no menu is left over from the old
    /// one.
    pub(crate) fn after_new_document(&mut self) {
        self.view = DocView::new(None);
        self.reset_views();
        self.picker = None;
        self.port_menu = None;
    }

    /// Brings the focused view's snarl graph in line with the document, if it changed.
    pub(crate) fn sync_view(&mut self) {
        self.view
            .sync(self.session.document(), self.session.catalog());
    }

    pub fn set_data_folder(&mut self, folder: DataFolder, choice: DataChoice) {
        self.session.set_data_folder(folder, choice);
    }

    pub fn settings_mut(&mut self) -> &mut SettingsWindow {
        &mut self.settings
    }

    pub fn data_choice(&self) -> Option<&DataChoice> {
        self.session.data_choice()
    }

    /// Loads another data version or language from the data folder. The open graph stays; its
    /// recipe nodes are matched against the new data by ID. A failure keeps the current data.
    pub fn switch_data(&mut self, choice: DataChoice, _strings: &Strings) {
        if self.session.switch_data(choice) {
            self.view
                .rebuild(self.session.document(), self.session.catalog());
            self.icons_stale = true;
        }
    }

    pub fn catalog(&self) -> &Catalog {
        self.session.catalog()
    }

    /// The open document with the view that draws it.
    pub fn document(&self) -> DocumentRef<'_> {
        DocumentRef {
            doc: self.session.document(),
            view: &self.view,
        }
    }

    /// The open document, for edits that keep the view in line.
    pub fn document_mut(&mut self) -> DocumentMut<'_> {
        DocumentMut {
            session: &mut self.session,
            view: &mut self.view,
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn availability(&self) -> &Availability {
        self.session.availability()
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.session.selection()
    }

    /// Selects these nodes, as a box selection would.
    pub fn select_nodes(&mut self, nodes: Vec<NodeId>) {
        self.session.select_nodes(nodes);
    }

    pub fn set_round_up(&mut self, on: bool) {
        self.session.set_round_up(on);
    }

    pub fn show_performance(&self) -> bool {
        self.show_performance
    }

    /// Whether the status bar shows the frame rate and the last solve's time.
    pub fn set_show_performance(&mut self, on: bool) {
        self.show_performance = on;
    }

    /// Opens the node picker as if from `origin`, with the new node going to `graph_pos`.
    pub fn open_picker(&mut self, origin: Origin, graph_pos: Pos2) {
        let screen = self.canvas.to_screen(graph_pos);
        self.picker = Some(Picker::new(origin, screen, graph_pos));
    }

    /// The open picker's egui area.
    pub fn picker_area(&self) -> Option<egui::Id> {
        self.picker.as_ref().map(Picker::area_id)
    }

    /// The origin of the open picker, if one is open.
    pub fn picker_origin(&self) -> Option<&Origin> {
        self.picker.as_ref().map(|picker| &picker.origin)
    }

    pub fn port_menu_is_open(&self) -> bool {
        self.port_menu.is_some()
    }

    pub fn picker_is_open(&self) -> bool {
        self.picker.is_some()
    }

    /// Where a port was drawn last frame, on screen.
    pub fn port_on_screen(&self, node: NodeId, side: Side, item: &str) -> Option<Pos2> {
        self.canvas.port_on_screen(
            &self.view,
            self.session.document(),
            self.session.catalog(),
            node,
            side,
            &item.into(),
        )
    }

    /// Where a node was drawn last frame, on screen.
    pub fn node_on_screen(&self, node: NodeId) -> Option<eframe::egui::Rect> {
        self.canvas.item_on_screen(Member::Node(node))
    }

    /// Where a workspace's card was drawn last frame, on screen.
    pub fn workspace_on_screen(&self, workspace: WorkspaceId) -> Option<eframe::egui::Rect> {
        self.canvas.item_on_screen(Member::Workspace(workspace))
    }

    /// Shows another level of the graph: a workspace, or `None` for the top. The selection and
    /// any open menu belong to the level left behind.
    pub fn show_level(&mut self, level: Option<WorkspaceId>) {
        if level == self.view.level() {
            return;
        }
        self.view
            .set_level(self.session.document(), self.session.catalog(), level);
        self.select(None);
        self.picker = None;
        self.port_menu = None;
    }

    /// Where the canvas is looking (see [`crate::canvas::Canvas::view`]).
    pub fn canvas_view(&self) -> Option<CanvasView> {
        self.canvas.view()
    }

    /// The middle of the canvas, on screen.
    pub fn canvas_center_on_screen(&self) -> Pos2 {
        self.canvas.rect.center()
    }

    /// Zooms the canvas to `zoom` (1 is 100%) around its middle, from the next frame.
    pub fn zoom_canvas_to(&mut self, zoom: f32) {
        self.canvas.zoom_to(zoom);
    }

    /// The top-left of the canvas, on screen: where [`Self::canvas_view`]'s point is.
    pub fn canvas_corner_on_screen(&self) -> Pos2 {
        self.canvas.rect.min
    }

    pub fn ui(&mut self, ui: &mut Ui, strings: &Strings) {
        // A frame egui runs twice is drawn once.
        if ui.ctx().current_pass_index() == 0 {
            self.frames.add(ui.input(|i| i.time), ());
        }
        self.handle_close_request(ui.ctx(), strings);
        self.poll_upload(ui.ctx(), strings);
        if std::mem::take(&mut self.icons_stale) {
            crate::widgets::forget_icons(ui.ctx());
        }
        egui::Panel::top("top-bar").show(ui, |ui| self.top_bar(ui, strings));
        egui::Panel::bottom("status-bar").show(ui, |ui| self.status_bar(ui, strings));
        let mut show_browser = self.show_browser;
        egui::Panel::left("recipe-browser")
            .resizable(true)
            .default_size(240.0)
            .show_collapsible(ui, &mut show_browser, |ui| {
                if let Some(kind) = self
                    .browser
                    .ui(ui, self.session.catalog(), strings)
                    .and_then(|id| recipe_node(self.session.catalog(), id))
                {
                    let at = to_pos(self.canvas.center());
                    let node = self.session.add_node(kind, at, self.view.level());
                    self.select(Some(Selection::node(node)));
                }
            });
        self.show_browser = show_browser;
        let mut show_inspector = self.show_inspector;
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(300.0)
            .show_collapsible(ui, &mut show_inspector, |ui| self.inspector(ui, strings));
        self.show_inspector = show_inspector;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.views_ui(ui, strings));
        self.view_windows(ui.ctx(), strings);
        self.finish_views(ui.ctx());
        if !self.views.is_in_window(self.views.focused()) {
            self.overlays_ui(ui.ctx(), strings);
        }
        self.save_as_prompt_ui(ui.ctx(), strings);
        let (catalog, availability, data) = self.session.settings_parts();
        let data = data.map(|(folder, choice)| (folder, choice.clone()));
        let switch = self
            .settings
            .show(ui.ctx(), catalog, strings, availability, data);
        if let Some(choice) = switch {
            self.switch_data(choice, strings);
        }
        self.keyboard(ui.ctx(), strings);
        self.finish_export(ui.ctx(), strings);
        self.end_undo_step(ui.ctx());
        self.autosave(ui.ctx(), strings);
        self.update_title(ui.ctx(), strings);
        if self.session.solve_if_stale().is_some() {
            ui.ctx().request_repaint();
        }
        self.sync_view();
    }

    /// Ends the undo step once the player lets go: a drag, or typing in a field, undoes in one go.
    fn end_undo_step(&mut self, ctx: &egui::Context) {
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.focused().is_some());
        if !busy {
            self.session.checkpoint();
        }
    }

    pub(crate) fn apply(&mut self, event: CanvasEvent, strings: &Strings) {
        let level = self.view.level();
        match event {
            CanvasEvent::Connect { from, to } => self.session.connect(from, to),
            CanvasEvent::Disconnect(link) => self.session.disconnect(link),
            CanvasEvent::OpenPicker { origin, graph_pos } => self.open_picker(origin, graph_pos),
            CanvasEvent::Select(selection) => self.select(selection),
            CanvasEvent::Toggle(member) => self.session.toggle(member),
            CanvasEvent::Remove(Member::Node(node)) => self.session.remove_node(node),
            CanvasEvent::Remove(Member::Workspace(workspace)) => {
                self.session.remove_workspace(workspace);
            }
            CanvasEvent::ShowLevel(level) => self.show_level(level),
            CanvasEvent::AddPort(side) => {
                self.session.add_port(level, side);
            }
            CanvasEvent::Rename(workspace, name) => {
                let _ = self.session.rename_workspace(workspace, &name);
            }
            CanvasEvent::Dissolve(workspace) => {
                self.select(Selection::of_items(vec![Member::Workspace(workspace)]));
                self.session.dissolve_selection(level);
            }
            CanvasEvent::Cut(member) => {
                self.select(Selection::of_items(vec![member]));
                self.run(Shortcut::Cut, strings);
            }
            CanvasEvent::Copy(member) => {
                self.select(Selection::of_items(vec![member]));
                self.session.copy();
            }
            CanvasEvent::PasteInto(workspace) => self.session.paste_into(Some(workspace), level),
            CanvasEvent::SetFacility(node, facility) => {
                let _ = self.session.set_facility(node, facility);
            }
            CanvasEvent::SetLimit(node, limit) => {
                let _ = self.session.set_limit(node, limit);
            }
            CanvasEvent::OpenPortMenu { port, graph_pos } => {
                let screen = port_menu_pos(self.canvas.to_screen(graph_pos), port.side);
                self.picker = None;
                self.port_menu = Some(PortMenu::new(port, screen));
            }
        }
    }

    pub(crate) fn select(&mut self, selection: Option<Selection>) {
        self.session.select(selection);
    }

    /// Adds the node the player picked and, when the picker was opened from a port, links it there.
    /// One click does all of it.
    pub fn add_from_picker(
        &mut self,
        origin: &Origin,
        graph_pos: Pos2,
        choice: Choice,
    ) -> Option<NodeId> {
        let (node, anchor) =
            self.session
                .add_from_picker(origin, to_pos(graph_pos), choice, self.view.level())?;
        self.sync_view();
        if let Some(anchor) = anchor {
            self.view.anchors.insert(node, anchor);
        }
        Some(node)
    }

    /// The node picker and the port menu, over the focused view, in the window that shows it.
    pub(crate) fn overlays_ui(&mut self, ctx: &egui::Context, strings: &Strings) {
        self.picker_ui(ctx, strings);
        self.port_menu_ui(ctx, strings);
    }

    fn picker_ui(&mut self, ctx: &egui::Context, strings: &Strings) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        match picker.show(ctx, self.session.catalog(), strings) {
            picker::Outcome::Open => {}
            picker::Outcome::Close => self.picker = None,
            picker::Outcome::Chosen(Choice::Workspace) => {
                let picker = self.picker.take().expect("picker is open");
                self.session
                    .add_workspace(to_pos(picker.graph_pos), self.view.level());
            }
            picker::Outcome::Chosen(choice) => {
                let picker = self.picker.take().expect("picker is open");
                self.add_from_picker(&picker.origin, picker.graph_pos, choice);
            }
        }
    }

    fn port_menu_ui(&mut self, ctx: &egui::Context, strings: &Strings) {
        let Some(menu) = &mut self.port_menu else {
            return;
        };
        let port = menu.port.clone();
        // Follow the pin: the card can grow, and the canvas can pan or zoom, while the menu is open.
        if let Some(pin) = self.canvas.port_on_screen(
            &self.view,
            self.session.document(),
            self.session.catalog(),
            port.node,
            port.side,
            &port.item,
        ) {
            menu.screen_pos = port_menu_pos(pin, port.side);
        }
        match menu.show(
            ctx,
            self.session.catalog(),
            strings,
            self.session.document().graph(),
        ) {
            port_menu::Outcome::Open => {}
            port_menu::Outcome::Close => self.port_menu = None,
            port_menu::Outcome::SetLimit(limit) => {
                let _ = self.session.set_port_limit(&port, limit);
            }
            port_menu::Outcome::SetShare(link, share) => {
                let _ = self.session.set_share(link, port.side, share);
            }
            port_menu::Outcome::ClearShares => {
                let links: Vec<_> = self
                    .session
                    .document()
                    .graph()
                    .links_at(port.node, port.side)
                    .filter(|(_, link)| link.item == port.item)
                    .map(|(id, _)| id)
                    .collect();
                for link in links {
                    let _ = self.session.set_share(link, port.side, None);
                }
            }
            port_menu::Outcome::ClearLinks => {
                self.session.clear_port(&port);
                self.port_menu = None;
            }
        }
    }

    fn top_bar(&mut self, ui: &mut Ui, strings: &Strings) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(strings.get("menu-file"), |ui| self.file_menu(ui, strings));
            ui.menu_button(strings.get("menu-edit"), |ui| self.edit_menu(ui, strings));
            ui.menu_button(strings.get("menu-view"), |ui| {
                ui.checkbox(&mut self.show_browser, strings.get("toggle-recipes"));
                ui.checkbox(&mut self.show_inspector, strings.get("toggle-inspector"));
                let mut round_up = self.session.round_up();
                ui.checkbox(&mut round_up, strings.get("menu-round-up"));
                self.session.set_round_up(round_up);
                ui.checkbox(&mut self.show_performance, strings.get("menu-performance"));
                ui.separator();
                let entries = [
                    Shortcut::SplitRight,
                    Shortcut::SplitDown,
                    Shortcut::DuplicateView,
                    Shortcut::MoveToWindow,
                    Shortcut::CloseView,
                ];
                for shortcut in entries {
                    if shortcut == Shortcut::MoveToWindow && !crate::views::WINDOWS {
                        continue;
                    }
                    let button = egui::Button::new(strings.get(shortcut.label_key()))
                        .shortcut_text(ui.ctx().format_shortcut(&crate::edit::keys(shortcut)));
                    if ui.add_enabled(self.can_run(shortcut), button).clicked() {
                        self.run(shortcut, strings);
                    }
                }
            });
            ui.separator();
            ui.toggle_value(&mut self.show_browser, strings.get("toggle-recipes"));
            ui.separator();
            self.toolbar(ui, strings);
            // Right to left: the last added sits leftmost.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut self.show_inspector, strings.get("toggle-inspector"));
                ui.separator();
                if ui.button(strings.get("settings-button")).clicked() {
                    self.settings
                        .open_on(&self.session.document().graph().region().clone());
                }
                let mut round_up = self.session.round_up();
                ui.toggle_value(&mut round_up, strings.get("toggle-round-up"))
                    .on_hover_text(strings.get("menu-round-up"));
                self.session.set_round_up(round_up);
                ui.separator();
                ui.weak(strings.format(
                    "data-version",
                    &[("version", self.session.catalog().version())],
                ));
                ui.separator();
                self.region_picker(ui, strings);
                ui.label(strings.get("region-label"));
            });
        });
    }

    /// The graph's region. It can change at any time: what the new region lacks shows as
    /// warnings on the cards, and undo takes it back.
    fn region_picker(&mut self, ui: &mut Ui, strings: &Strings) {
        let current = self.session.document().graph().region().clone();
        let mut chosen = None;
        let picker = egui::ComboBox::from_id_salt("region")
            .selected_text(names::region(self.session.catalog(), current.as_str()))
            .show_ui(ui, |ui| {
                for region in self.session.catalog().regions().keys() {
                    let name = names::region(self.session.catalog(), region.as_str());
                    if ui.selectable_label(*region == current, name).clicked() {
                        chosen = Some(region.clone());
                    }
                }
            });
        let label = strings.get("region-label");
        ui.ctx()
            .accesskit_node_builder(picker.response.id, |node| node.set_label(label));
        if let Some(region) = chosen.filter(|r| *r != current) {
            self.session.set_region(region);
        }
    }

    fn file_menu(&mut self, ui: &mut Ui, strings: &Strings) {
        let item = |ui: &mut Ui, key: &str, shortcut: Option<Shortcut>| {
            let mut button = egui::Button::new(strings.get(key));
            if let Some(shortcut) = shortcut {
                button =
                    button.shortcut_text(ui.ctx().format_shortcut(&crate::edit::keys(shortcut)));
            }
            ui.add(button).clicked()
        };
        if item(ui, "menu-new", Some(Shortcut::New)) {
            self.request(Pending::New, strings);
        }
        if item(ui, "menu-open", Some(Shortcut::Open)) {
            self.request(Pending::Open, strings);
        }
        ui.add_enabled_ui(!self.session.recent_files().is_empty(), |ui| {
            ui.menu_button(strings.get("menu-open-recent"), |ui| {
                let mut chosen = None;
                for path in self.session.recent_files() {
                    let name = self.session.file_label(path);
                    if ui.button(name).clicked() {
                        chosen = Some(path.clone());
                    }
                }
                if let Some(path) = chosen {
                    self.request(Pending::OpenPath(path), strings);
                }
            });
        });
        ui.separator();
        if item(ui, "menu-save", Some(Shortcut::Save)) {
            self.save(strings);
        }
        if item(ui, "menu-save-as", Some(Shortcut::SaveAs)) {
            self.save_as(strings);
        }
        ui.separator();
        if item(ui, "menu-export-png", None) {
            ui.close();
            self.export_png(strings);
        }
    }

    fn edit_menu(&mut self, ui: &mut Ui, strings: &Strings) {
        let entries = [
            Shortcut::Undo,
            Shortcut::Redo,
            Shortcut::Cut,
            Shortcut::Copy,
            Shortcut::Paste,
            Shortcut::Duplicate,
            Shortcut::Delete,
            Shortcut::SelectAll,
            Shortcut::MakeWorkspace,
            Shortcut::Dissolve,
        ];
        for shortcut in entries {
            let button = egui::Button::new(strings.get(shortcut.label_key()))
                .shortcut_text(ui.ctx().format_shortcut(&crate::edit::keys(shortcut)));
            if ui.add_enabled(self.can_run(shortcut), button).clicked() {
                self.run(shortcut, strings);
            }
        }
    }

    fn status_bar(&mut self, ui: &mut Ui, strings: &Strings) {
        ui.horizontal(|ui| {
            if let Some(version) = self.session.other_data_version() {
                let text = strings.format(
                    "file-other-data-version",
                    &[
                        ("file", version),
                        ("active", self.session.catalog().version()),
                    ],
                );
                ui.label(RichText::new(text).color(colors::WARN));
                ui.separator();
            }
            if let Some(status) = self.session.status() {
                ui.label(RichText::new(status).color(colors::WARN));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.save_indicator(ui, strings);
                ui.separator();
                self.zoom_control(ui, strings);
                if self.show_performance {
                    ui.separator();
                    self.performance(ui, strings);
                }
            });
        });
    }

    /// `60 fps · Solve 0.4 ms`, laid out right to left. Solving runs on the UI thread, so a solve
    /// slow enough to drop frames shows as a warning; that is the point to move it to a worker
    /// (PLAN.md, M4).
    fn performance(&self, ui: &mut Ui, strings: &Strings) {
        if let Some(took) = self.session.last_solve() {
            let ms = format::short(Some(took.as_secs_f64() * 1000.0));
            let text = RichText::new(strings.format("status-solve-time", &[("ms", &ms)]));
            if took > SLOW_SOLVE {
                ui.label(text.color(colors::WARN))
                    .on_hover_text(strings.get("status-slow-solve"));
            } else {
                ui.label(text);
            }
        }
        if let Some(rate) = self.frames.rate() {
            let fps = format!("{rate:.0}");
            ui.label(strings.format("status-frame-rate", &[("fps", &fps)]));
        }
    }

    /// `− ──●── + 100%`: the canvas's zoom, set by the slider or stepped by the buttons. Laid out
    /// right to left, so the percentage comes first.
    fn zoom_control(&mut self, ui: &mut Ui, strings: &Strings) {
        let Some(view) = self.canvas.view() else {
            return;
        };
        let percent = |zoom: f32| {
            let percent = (zoom * 100.0).round().to_string();
            egui::WidgetText::from(strings.format("zoom-percent", &[("percent", &percent)]))
        };
        // As wide as the widest, so the slider never shifts under the pointer while it drags.
        let widest = percent(canvas::MAX_ZOOM)
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Body,
            )
            .size()
            .ceil();
        ui.add_sized(widest, egui::Label::new(percent(view.zoom)));
        let step = |ui: &mut Ui, text: &str, label: String| {
            let response = ui.add(egui::Button::new(text).frame(false));
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label(label));
            response.clicked()
        };
        let mut zoom = view.zoom;
        if step(ui, "+", strings.get("zoom-in")) {
            zoom = canvas::zoom_step(view.zoom, true);
        }
        ui.spacing_mut().slider_width = ZOOM_SLIDER_WIDTH;
        let slider = ui.add(
            egui::Slider::new(&mut zoom, canvas::MIN_ZOOM..=canvas::MAX_ZOOM)
                .step_by(f64::from(canvas::ZOOM_STEP))
                .show_value(false),
        );
        let label = strings.get("zoom-label");
        ui.ctx()
            .accesskit_node_builder(slider.id, |node| node.set_label(label));
        if step(ui, "−", strings.get("zoom-out")) {
            zoom = canvas::zoom_step(view.zoom, false);
        }
        if zoom != view.zoom {
            self.canvas.zoom_to(zoom);
        }
    }
}

/// Where the port menu's top-left goes for a pin on screen: beside the pin, away from its node.
fn port_menu_pos(pin: Pos2, side: Side) -> Pos2 {
    let x = match side {
        Side::Input => pin.x - port_menu::WIDTH - PORT_MENU_GAP,
        Side::Output => pin.x + PORT_MENU_GAP,
    };
    Pos2::new(x, pin.y - PORT_MENU_GAP)
}
