//! Everything on screen once game data has loaded: the menu bar, the recipe browser, the canvas, the
//! inspector, the node picker, the dialogs and the settings window.

use std::path::PathBuf;
use std::time::Duration;

use aic_core::{
    CanvasView, Graph, LinkError, Member, NodeId, NodeKind, PortRef, Side, WorkspaceId,
};
use aic_data::{Catalog, RecipeId, RegionId};
use eframe::egui::{self, Pos2, RichText, Ui, vec2};

use crate::canvas::{Canvas, CanvasEvent, Selection};
use crate::data::{DataChoice, DataFolder};
use crate::document::{Anchor, Document, port_ref};
use crate::edit::Shortcut;
use crate::files::Pending;
use crate::format;
use crate::i18n::Strings;
use crate::names;
use crate::picker::{self, Choice, Origin, Picker};
use crate::port_menu::{self, PortMenu};
use crate::settings::{Availability, SettingsWindow};
use crate::throttle::Throttle;
use crate::widgets::colors;

/// A solve slower than this is reported in the status bar.
const SLOW_SOLVE: Duration = Duration::from_millis(16);
/// Space between a port's pin and the menu it opens.
const PORT_MENU_GAP: f32 = 12.0;

pub struct Editor {
    pub(crate) catalog: Catalog,
    pub(crate) doc: Document,
    pub(crate) canvas: Canvas,
    pub(crate) picker: Option<Picker>,
    pub(crate) selection: Option<Selection>,
    pub(crate) availability: Availability,
    pub(crate) settings: SettingsWindow,
    pub(crate) browser: crate::browser::Browser,
    /// What the player is typing in the inspector's limit field.
    pub(crate) limit_edit: String,
    pub(crate) port_menu: Option<PortMenu>,
    pub(crate) show_browser: bool,
    pub(crate) show_inspector: bool,
    /// Show whole facilities and how busy they are instead of fractional counts.
    pub(crate) round_up: bool,
    /// Nodes copied with Ctrl+C, and how many times they were pasted since.
    pub(crate) clipboard: Option<(Graph, u32)>,
    /// Files opened or saved lately, newest first. Kept in eframe storage.
    pub(crate) recent_files: Vec<PathBuf>,
    /// Where drafts are saved; `None` keeps new graphs in memory until Save As.
    pub(crate) drafts_dir: Option<PathBuf>,
    pub(crate) autosave: Throttle,
    /// The canvas view as last saved, so moving the view also gets saved.
    pub(crate) saved_view: Option<CanvasView>,
    /// The data version of the open file, when it isn't the active one.
    pub(crate) other_data_version: Option<String>,
    /// The data folder and what was loaded from it; `None` when the app was given a catalog.
    pub(crate) data: Option<(DataFolder, DataChoice)>,
    /// Decoded icons belong to the old data after a switch.
    pub(crate) icons_stale: bool,
    /// The shortcut last pressed, and when: its toolbar button lights up for a moment.
    pub(crate) flash: Option<(Shortcut, std::time::Instant)>,
    /// When the document was last written, and whether the last try failed.
    pub(crate) last_saved: Option<std::time::Instant>,
    pub(crate) save_failed: bool,
    /// Where to save the canvas once the window is next drawn.
    pub(crate) export_to: Option<PathBuf>,
    /// The last refused action or file message, shown until the next one.
    pub(crate) status: Option<String>,
    last_solve: Option<Duration>,
    pub(crate) last_title: String,
}

impl Editor {
    pub fn new(catalog: Catalog, availability: Availability) -> Self {
        let region = catalog
            .regions()
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| RegionId::from(""));
        Self {
            catalog,
            doc: Document::new(region),
            canvas: Canvas::default(),
            picker: None,
            selection: None,
            availability,
            settings: SettingsWindow::default(),
            browser: crate::browser::Browser::default(),
            limit_edit: String::new(),
            port_menu: None,
            show_browser: true,
            show_inspector: false,
            round_up: false,
            clipboard: None,
            recent_files: Vec::new(),
            drafts_dir: None,
            autosave: Throttle::new(crate::files::AUTOSAVE_INTERVAL),
            saved_view: None,
            other_data_version: None,
            data: None,
            icons_stale: false,
            export_to: None,
            flash: None,
            last_saved: None,
            save_failed: false,
            status: None,
            last_solve: None,
            last_title: String::new(),
        }
    }

    /// Starts an empty graph in `region`, dropping the open one.
    pub fn new_graph(&mut self, region: RegionId) {
        self.set_document(Document::new(region));
        self.other_data_version = None;
    }

    pub(crate) fn set_document(&mut self, doc: Document) {
        self.doc = doc;
        self.picker = None;
        self.port_menu = None;
        self.select(None);
    }

    pub fn set_data_folder(&mut self, folder: DataFolder, choice: DataChoice) {
        self.data = Some((folder, choice));
    }

    pub fn settings_mut(&mut self) -> &mut SettingsWindow {
        &mut self.settings
    }

    pub fn data_choice(&self) -> Option<&DataChoice> {
        self.data.as_ref().map(|(_, choice)| choice)
    }

    /// Loads another data version or language from the data folder. The open graph stays; its
    /// recipe nodes are matched against the new data by ID. A failure keeps the current data.
    pub fn switch_data(&mut self, choice: DataChoice, strings: &Strings) {
        let Some((folder, _)) = &self.data else {
            return;
        };
        match folder.load(&choice) {
            Ok(catalog) => {
                self.doc.rebind(&catalog);
                self.catalog = catalog;
                self.icons_stale = true;
                if let Some((_, current)) = &mut self.data {
                    *current = choice;
                }
                self.status = None;
            }
            Err(error) => {
                self.status =
                    Some(strings.format("data-switch-failed", &[("error", &error.to_string())]));
            }
        }
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.doc
    }

    pub fn availability(&self) -> &Availability {
        &self.availability
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Selects these nodes, as a box selection would.
    pub fn select_nodes(&mut self, nodes: Vec<NodeId>) {
        self.select(Selection::of_nodes(nodes));
    }

    pub fn set_round_up(&mut self, on: bool) {
        self.round_up = on;
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
        self.canvas
            .port_on_screen(&self.doc, &self.catalog, node, side, &item.into())
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
        if level == self.doc.level() {
            return;
        }
        self.doc.set_level(&self.catalog, level);
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

    pub fn ui(&mut self, ui: &mut Ui, strings: &Strings) {
        self.handle_close_request(ui.ctx(), strings);
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
                let region = self.doc.graph().region();
                if let Some(kind) = self
                    .browser
                    .ui(ui, &self.catalog, strings, region)
                    .and_then(|id| recipe_node(&self.catalog, id))
                {
                    let node = self.doc.add_node(kind, self.canvas.center());
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
            .show(ui, |ui| {
                let events = self.canvas.show(
                    ui,
                    &mut self.doc,
                    &self.catalog,
                    strings,
                    self.selection.as_ref(),
                    self.round_up,
                );
                for event in events {
                    self.apply(event, strings);
                }
            });
        self.doc.sync_from_view(&self.catalog);
        self.picker_ui(ui.ctx(), strings);
        self.port_menu_ui(ui.ctx(), strings);
        let data = self
            .data
            .as_ref()
            .map(|(folder, choice)| (folder, choice.clone()));
        let switch = self.settings.show(
            ui.ctx(),
            &self.catalog,
            strings,
            &mut self.availability,
            data,
        );
        if let Some(choice) = switch {
            self.switch_data(choice, strings);
        }
        self.keyboard(ui.ctx(), strings);
        self.finish_export(ui.ctx(), strings);
        self.end_undo_step(ui.ctx());
        self.autosave(ui.ctx(), strings);
        self.update_title(ui.ctx(), strings);
        if let Some(took) = self.doc.solve_if_stale(&self.catalog) {
            self.last_solve = Some(took);
            ui.ctx().request_repaint();
        }
    }

    /// Ends the undo step once the player lets go: a drag, or typing in a field, undoes in one go.
    fn end_undo_step(&mut self, ctx: &egui::Context) {
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.focused().is_some());
        if !busy {
            self.doc.checkpoint();
        }
    }

    fn apply(&mut self, event: CanvasEvent, strings: &Strings) {
        match event {
            CanvasEvent::Connect { from, to } => self.connect(from, to, strings),
            CanvasEvent::Disconnect(link) => {
                self.doc.remove_link(&self.catalog, link);
                if self.selection == Some(Selection::Link(link)) {
                    self.select(None);
                }
            }
            CanvasEvent::OpenPicker { origin, graph_pos } => self.open_picker(origin, graph_pos),
            CanvasEvent::Select(selection) => self.select(selection),
            CanvasEvent::Toggle(member) => {
                let mut items = self
                    .selection
                    .as_ref()
                    .map(|s| s.items().to_vec())
                    .unwrap_or_default();
                if let Some(at) = items.iter().position(|m| *m == member) {
                    items.remove(at);
                } else {
                    items.push(member);
                }
                self.select(Selection::of_items(items));
            }
            CanvasEvent::Remove(Member::Node(node)) => self.remove_node(node),
            CanvasEvent::Remove(Member::Workspace(workspace)) => {
                self.doc.remove_workspace(&self.catalog, workspace);
                self.select(None);
            }
            CanvasEvent::ShowLevel(level) => self.show_level(level),
            CanvasEvent::AddPort(side) => {
                if let Some(port) = self.doc.add_port(&self.catalog, side) {
                    self.select(Some(Selection::node(port)));
                }
            }
            CanvasEvent::Rename(workspace, name) => {
                let _ = self.doc.rename_workspace(workspace, &name);
            }
            CanvasEvent::Dissolve(workspace) => {
                self.select(Selection::of_items(vec![Member::Workspace(workspace)]));
                self.dissolve_selection();
            }
            CanvasEvent::Cut(member) => {
                self.select(Selection::of_items(vec![member]));
                self.run(Shortcut::Cut, strings);
            }
            CanvasEvent::Copy(member) => {
                self.select(Selection::of_items(vec![member]));
                self.copy();
            }
            CanvasEvent::PasteInto(workspace) => self.paste_into(Some(workspace)),
            CanvasEvent::SetFacility(node, facility) => {
                let _ = self.doc.set_facility(node, facility);
            }
            CanvasEvent::SetLimit(node, limit) => {
                let _ = self.doc.set_limit(node, limit);
            }
            CanvasEvent::OpenPortMenu { port, graph_pos } => {
                let screen = port_menu_pos(self.canvas.to_screen(graph_pos), port.side);
                self.picker = None;
                self.port_menu = Some(PortMenu::new(port, screen));
            }
        }
    }

    pub(crate) fn select(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    pub(crate) fn remove_node(&mut self, node: NodeId) {
        self.doc.remove_node(&self.catalog, node);
        if let Some(selection) = &self.selection
            && selection.contains_node(node)
        {
            let rest: Vec<Member> = selection
                .items()
                .iter()
                .copied()
                .filter(|m| *m != Member::Node(node))
                .collect();
            self.select(Selection::of_items(rest));
        }
    }

    fn connect(&mut self, from: PortRef, to: PortRef, strings: &Strings) {
        match self.doc.connect(&self.catalog, from, to) {
            Ok(_) => self.status = None,
            Err(error) => {
                let reason = link_error(&self.catalog, strings, &error);
                self.status = Some(strings.format("link-refused", &[("error", &reason)]));
            }
        }
    }

    /// Adds the node the player picked and, when the picker was opened from a port, links it there.
    /// One click does all of it.
    pub fn add_from_picker(
        &mut self,
        origin: &Origin,
        graph_pos: Pos2,
        choice: Choice,
    ) -> Option<NodeId> {
        let item = match origin {
            Origin::Port { item, .. } => Some(item),
            Origin::Canvas => None,
        };
        let kind = match choice {
            Choice::Recipe(recipe) => recipe_node(&self.catalog, recipe)?,
            Choice::Builtin(builtin) => picker::builtin_kind(&self.catalog, builtin, item),
            Choice::Workspace => return None,
        };
        // Roughly line the new node's first port up with the point.
        let pos = graph_pos - vec2(0.0, 40.0);
        let node = match origin {
            Origin::Canvas => self.doc.add_node(kind, pos),
            Origin::Port { side, .. } => {
                let anchor = match side {
                    Side::Input => Anchor::Right(graph_pos.x),
                    Side::Output => Anchor::Left(graph_pos.x),
                };
                self.doc.add_node_anchored(kind, pos, anchor)
            }
        };
        if let Origin::Port {
            node: other,
            side,
            item,
        } = origin
        {
            let graph = self.doc.graph();
            let new_kind = &graph.node(node)?.kind;
            let other_kind = &graph.node(*other)?.kind;
            let (from, to) = match side {
                Side::Input => (
                    port_ref(new_kind, node, Some(item)),
                    port_ref(other_kind, *other, Some(item)),
                ),
                Side::Output => (
                    port_ref(other_kind, *other, Some(item)),
                    port_ref(new_kind, node, Some(item)),
                ),
            };
            // The picker offers only nodes that fit the port, so this link can't be refused.
            let linked = self.doc.connect(&self.catalog, from, to);
            debug_assert!(
                linked.is_ok(),
                "picker offered a node that doesn't fit: {linked:?}"
            );
        }
        self.select(Some(Selection::node(node)));
        Some(node)
    }

    fn picker_ui(&mut self, ctx: &egui::Context, strings: &Strings) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        let region = self.doc.graph().region().clone();
        match picker.show(ctx, &self.catalog, strings, &region) {
            picker::Outcome::Open => {}
            picker::Outcome::Close => self.picker = None,
            picker::Outcome::Chosen(Choice::Workspace) => {
                let picker = self.picker.take().expect("picker is open");
                let name = self.new_workspace_name(strings);
                let workspace = self
                    .doc
                    .add_workspace(&self.catalog, &name, picker.graph_pos);
                self.select(Selection::of_items(vec![Member::Workspace(workspace)]));
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
        if let Some(pin) =
            self.canvas
                .port_on_screen(&self.doc, &self.catalog, port.node, port.side, &port.item)
        {
            menu.screen_pos = port_menu_pos(pin, port.side);
        }
        match menu.show(ctx, &self.catalog, strings, self.doc.graph()) {
            port_menu::Outcome::Open => {}
            port_menu::Outcome::Close => self.port_menu = None,
            port_menu::Outcome::SetLimit(limit) => {
                let _ = self.doc.set_port_limit(&self.catalog, &port, limit);
            }
            port_menu::Outcome::SetShare(link, share) => {
                let _ = self.doc.set_share(link, port.side, share);
            }
            port_menu::Outcome::ClearShares => {
                let links: Vec<_> = self
                    .doc
                    .graph()
                    .links_at(port.node, port.side)
                    .filter(|(_, link)| link.item == port.item)
                    .map(|(id, _)| id)
                    .collect();
                for link in links {
                    let _ = self.doc.set_share(link, port.side, None);
                }
            }
            port_menu::Outcome::ClearLinks => {
                self.doc.clear_port(&self.catalog, &port);
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
                ui.checkbox(&mut self.round_up, strings.get("menu-round-up"));
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
                    self.settings.open_on(&self.doc.graph().region().clone());
                }
                ui.toggle_value(&mut self.round_up, strings.get("toggle-round-up"))
                    .on_hover_text(strings.get("menu-round-up"));
                ui.separator();
                ui.weak(strings.format("data-version", &[("version", self.catalog.version())]));
                ui.separator();
                self.region_picker(ui, strings);
                ui.label(strings.get("region-label"));
            });
        });
    }

    /// The graph's region, which only an empty graph can change: File > New, then pick.
    fn region_picker(&mut self, ui: &mut Ui, strings: &Strings) {
        let current = self.doc.graph().region().clone();
        let empty = self.doc.graph().nodes().is_empty();
        let mut chosen = None;
        ui.add_enabled_ui(empty, |ui| {
            let picker = egui::ComboBox::from_id_salt("region")
                .selected_text(names::region(&self.catalog, current.as_str()))
                .show_ui(ui, |ui| {
                    for region in self.catalog.regions().keys() {
                        let name = names::region(&self.catalog, region.as_str());
                        if ui.selectable_label(*region == current, name).clicked() {
                            chosen = Some(region.clone());
                        }
                    }
                });
            let label = strings.get("region-label");
            ui.ctx()
                .accesskit_node_builder(picker.response.id, |node| node.set_label(label));
        })
        .response
        .on_disabled_hover_text(strings.get("region-locked"));
        if let Some(region) = chosen.filter(|r| *r != current) {
            let _ = self.doc.set_region(region);
        }
    }

    fn file_menu(&mut self, ui: &mut Ui, strings: &Strings) {
        let item = |ui: &mut Ui, key: &str, shortcut: Option<Shortcut>| {
            let mut button = egui::Button::new(strings.get(key));
            if let Some(shortcut) = shortcut {
                button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut.keys()));
            }
            ui.add(button).clicked()
        };
        if item(ui, "menu-new", Some(Shortcut::New)) {
            self.request(Pending::New, strings);
        }
        if item(ui, "menu-open", Some(Shortcut::Open)) {
            self.request(Pending::Open, strings);
        }
        ui.add_enabled_ui(!self.recent_files.is_empty(), |ui| {
            ui.menu_button(strings.get("menu-open-recent"), |ui| {
                let mut chosen = None;
                for path in &self.recent_files {
                    let name = path.display().to_string();
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
            let button = egui::Button::new(strings.get(shortcut.name_key()))
                .shortcut_text(ui.ctx().format_shortcut(&shortcut.keys()));
            if ui.add_enabled(self.can_run(shortcut), button).clicked() {
                self.run(shortcut, strings);
            }
        }
    }

    fn status_bar(&self, ui: &mut Ui, strings: &Strings) {
        ui.horizontal(|ui| {
            if let Some(version) = &self.other_data_version {
                let text = strings.format(
                    "file-other-data-version",
                    &[("file", version), ("active", self.catalog.version())],
                );
                ui.label(RichText::new(text).color(colors::WARN));
                ui.separator();
            }
            // Solving runs on the UI thread. Say so when it gets slow enough to drop frames; that is
            // the point to move it to a worker (PLAN.md, M4).
            if let Some(took) = self.last_solve
                && took > SLOW_SOLVE
            {
                let ms = format::short(Some(took.as_secs_f64() * 1000.0));
                ui.label(
                    RichText::new(strings.format("status-slow-solve", &[("ms", &ms)]))
                        .color(colors::WARN),
                );
                ui.separator();
            }
            if let Some(status) = &self.status {
                ui.label(RichText::new(status).color(colors::WARN));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.save_indicator(ui, strings);
            });
        });
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

/// A new recipe node: on the recipe's first facility, with no limit, so the solver sizes it from the
/// rest of the graph. A settlement trade becomes an outpost order.
fn recipe_node(catalog: &Catalog, recipe: RecipeId) -> Option<NodeKind> {
    if let Some(order) = NodeKind::outpost_order(catalog, &recipe) {
        return Some(order);
    }
    let facility = catalog.recipe(recipe.as_str())?.facilities.first()?.clone();
    Some(NodeKind::Recipe {
        recipe,
        facility,
        limit: None,
    })
}

/// Why a link was refused, in the player's words.
pub(crate) fn link_error(catalog: &Catalog, strings: &Strings, error: &LinkError) -> String {
    let item = |id: &aic_data::ItemId| names::item(catalog, id.as_str()).to_owned();
    match error {
        LinkError::ItemMismatch { from, to } => strings.format(
            "link-error-mismatch",
            &[("from", &item(from)), ("to", &item(to))],
        ),
        LinkError::SolidFluid { from, to } => strings.format(
            "link-error-solid-fluid",
            &[("from", &item(from)), ("to", &item(to))],
        ),
        LinkError::NoItem => strings.get("link-error-no-item"),
        LinkError::SelfLink => strings.get("link-error-self"),
        LinkError::Duplicate => strings.get("link-error-duplicate"),
        LinkError::AcrossWorkspaces => strings.get("link-error-across-workspaces"),
        LinkError::UnknownNode(_) | LinkError::NoSuchPort { .. } => strings.get("link-error-other"),
    }
}
