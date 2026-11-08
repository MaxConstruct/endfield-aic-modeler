//! Everything on screen once game data has loaded: the top bar, the recipe browser, the canvas, the
//! inspector, the node picker and the settings window.

use std::time::Duration;

use aic_core::{LinkError, NodeId, NodeKind, PortRef, Side};
use aic_data::{Catalog, RecipeId, RegionId};
use eframe::egui::{self, Key, Pos2, RichText, Ui, vec2};

use crate::canvas::{Canvas, CanvasEvent, Selection};
use crate::document::{Anchor, Document, port_ref};
use crate::format;
use crate::i18n::Strings;
use crate::names;
use crate::picker::{self, Choice, Origin, Picker};
use crate::settings::{Availability, SettingsWindow};
use crate::widgets::colors;

/// A solve slower than this is reported in the status bar.
const SLOW_SOLVE: Duration = Duration::from_millis(16);

pub struct Workspace {
    pub(crate) catalog: Catalog,
    pub(crate) doc: Document,
    pub(crate) canvas: Canvas,
    pub(crate) picker: Option<Picker>,
    pub(crate) selection: Option<Selection>,
    pub(crate) availability: Availability,
    pub(crate) settings: SettingsWindow,
    pub(crate) browser: crate::browser::Browser,
    /// The limit field of the selected node, kept while it doesn't parse.
    pub(crate) limit_edit: Option<(NodeId, String)>,
    /// The last refused action, shown until the next edit.
    status: Option<String>,
    last_solve: Option<Duration>,
}

impl Workspace {
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
            limit_edit: None,
            status: None,
            last_solve: None,
        }
    }

    /// Starts an empty graph in `region`.
    pub fn new_graph(&mut self, region: RegionId) {
        self.doc = Document::new(region);
        self.picker = None;
        self.select(None);
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

    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// Opens the node picker as if from `origin`, with the new node going to `graph_pos`.
    pub fn open_picker(&mut self, origin: Origin, graph_pos: Pos2) {
        let screen = self.canvas.to_screen(graph_pos);
        self.picker = Some(Picker::new(origin, screen, graph_pos));
    }

    /// The origin of the open picker, if one is open.
    pub fn picker_origin(&self) -> Option<&Origin> {
        self.picker.as_ref().map(|picker| &picker.origin)
    }

    pub fn picker_is_open(&self) -> bool {
        self.picker.is_some()
    }

    /// Where a port was drawn last frame, on screen.
    pub fn port_on_screen(&self, node: NodeId, side: Side, item: &str) -> Option<Pos2> {
        self.canvas
            .port_on_screen(&self.doc, &self.catalog, node, side, &item.into())
    }

    /// The middle of the canvas, on screen.
    pub fn canvas_center_on_screen(&self) -> Pos2 {
        self.canvas.rect.center()
    }

    pub fn ui(&mut self, ui: &mut Ui, strings: &Strings) {
        egui::Panel::top("top-bar").show(ui, |ui| self.top_bar(ui, strings));
        egui::Panel::bottom("status-bar").show(ui, |ui| self.status_bar(ui, strings));
        egui::Panel::left("recipe-browser")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                let region = self.doc.graph().region();
                if let Some(kind) = self
                    .browser
                    .ui(ui, &self.catalog, strings, region)
                    .and_then(|id| recipe_node(&self.catalog, id))
                {
                    let node = self.doc.add_node(kind, self.canvas.center());
                    self.select(Some(Selection::Node(node)));
                }
            });
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| self.inspector(ui, strings));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let events =
                    self.canvas
                        .show(ui, &mut self.doc, &self.catalog, strings, self.selection);
                for event in events {
                    self.apply(event, strings);
                }
            });
        self.doc.sync_from_view(&self.catalog);
        self.picker_ui(ui.ctx(), strings);
        self.settings
            .show(ui.ctx(), &self.catalog, strings, &mut self.availability);
        self.keyboard(ui.ctx());
        if let Some(took) = self.doc.solve_if_stale(&self.catalog) {
            self.last_solve = Some(took);
            ui.ctx().request_repaint();
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
            CanvasEvent::RemoveNode(node) => self.remove_node(node),
            CanvasEvent::SetFacility(node, facility) => {
                let _ = self.doc.set_facility(node, facility);
            }
        }
    }

    pub(crate) fn select(&mut self, selection: Option<Selection>) {
        if self.selection != selection {
            self.limit_edit = None;
        }
        self.selection = selection;
    }

    pub(crate) fn remove_node(&mut self, node: NodeId) {
        self.doc.remove_node(node);
        if self.selection == Some(Selection::Node(node)) {
            self.select(None);
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
        self.select(Some(Selection::Node(node)));
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
            picker::Outcome::Chosen(choice) => {
                let picker = self.picker.take().expect("picker is open");
                self.add_from_picker(&picker.origin, picker.graph_pos, choice);
            }
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some());
        if typing || self.picker.is_some() {
            return;
        }
        let delete = ctx.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace));
        if delete {
            match self.selection {
                Some(Selection::Node(node)) => self.remove_node(node),
                Some(Selection::Link(link)) => {
                    self.doc.remove_link(&self.catalog, link);
                    self.select(None);
                }
                None => {}
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.select(None);
        }
    }

    fn top_bar(&mut self, ui: &mut Ui, strings: &Strings) {
        ui.horizontal(|ui| {
            ui.strong(strings.get("app-title"));
            ui.separator();
            ui.label(strings.get("region-label"));
            let current = self.doc.graph().region().clone();
            let empty = self.doc.graph().nodes().is_empty();
            let mut chosen = None;
            ui.add_enabled_ui(empty, |ui| {
                egui::ComboBox::from_id_salt("region")
                    .selected_text(names::region(&self.catalog, current.as_str()))
                    .show_ui(ui, |ui| {
                        for region in self.catalog.regions().keys() {
                            let name = names::region(&self.catalog, region.as_str());
                            if ui.selectable_label(*region == current, name).clicked() {
                                chosen = Some(region.clone());
                            }
                        }
                    });
            })
            .response
            .on_disabled_hover_text(strings.get("region-locked"));
            if let Some(region) = chosen
                && region != current
            {
                self.new_graph(region);
            }
            ui.separator();
            ui.weak(strings.format("data-version", &[("version", self.catalog.version())]));
            ui.separator();
            if ui.button(strings.get("settings-button")).clicked() {
                self.settings.open_on(&self.doc.graph().region().clone());
            }
        });
    }

    fn status_bar(&self, ui: &mut Ui, strings: &Strings) {
        ui.horizontal(|ui| {
            // Solving runs on the UI thread. Say so when it gets slow enough to drop frames; that is
            // the point to move it to a worker (PLAN.md, M4).
            if let Some(took) = self.last_solve
                && took > SLOW_SOLVE
            {
                let ms = format::fixed(Some(took.as_secs_f64() * 1000.0));
                ui.label(
                    RichText::new(strings.format("status-slow-solve", &[("ms", &ms)]))
                        .color(colors::WARN),
                );
                ui.separator();
            }
            if let Some(status) = &self.status {
                ui.label(RichText::new(status).color(colors::WARN));
            }
        });
    }
}

/// A new recipe node: on the recipe's first facility, with no limit, so the solver sizes it from the
/// rest of the graph.
fn recipe_node(catalog: &Catalog, recipe: RecipeId) -> Option<NodeKind> {
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
        LinkError::UnknownNode(_) | LinkError::NoSuchPort { .. } => strings.get("link-error-other"),
    }
}
