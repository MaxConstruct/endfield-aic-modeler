//! One editing session: the catalog, the open [`Document`], the selection, the app clipboard and
//! the player's raw-resource limits, and every edit that isn't drawing. A UI draws what the session
//! holds, turns what the player does into calls on it, and finishes the [`Outcome`]s that need the
//! toolkit (dialogs, the system clipboard, view layout).
//!
//! What depends on a level of the graph takes it as a parameter ([`Level`]): views are the UI's
//! layout (ADR 0008).

use std::path::Path;
use std::time::Duration;

use aic_core::{
    EditError, Graph, LinkError, LinkId, Member, NodeId, NodeKind, Port, PortRef, Pos, Side,
    WorkspaceId,
};
use aic_data::{Catalog, FacilityId, RegionId};

use crate::command::Command;
use crate::data::{DataChoice, DataFolder};
use crate::document::{Anchor, Document, Level, port_ref};
use crate::files::Files;
use crate::i18n::Strings;
use crate::messages::link_error;
use crate::picker::{self, Choice, Origin};
use crate::selection::Selection;
use crate::settings::{Availability, Settings};

/// How far each paste lands from the copied nodes, in graph units.
const PASTE_OFFSET: f32 = 40.0;

/// A file command the session leaves to the UI, which owns the dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCommand {
    New,
    Open,
    Save,
    SaveAs,
}

/// A command about views, which are the UI's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewCommand {
    SplitRight,
    SplitDown,
    Duplicate,
    MoveToWindow,
    Close,
}

/// What a UI must do after [`Session::run`] when the session couldn't finish a command itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Done; the document, selection or status may have changed.
    Done,
    /// A file command: the UI asks for a path, or saves, and calls back in.
    File(FileCommand),
    /// Text for the system clipboard: the copied nodes as a save file.
    WriteClipboard(String),
    /// The system clipboard is the UI's to read: it calls [`Session::paste_text`] with what it
    /// finds.
    ReadClipboard,
    /// An undo or redo replaced the graph: what a popup or menu pointed at may be gone.
    Restored,
    /// A view command, for the UI's layout.
    View(ViewCommand),
    /// There was nothing to do.
    Nothing,
}

/// What [`Session::apply_settings`] leaves to the UI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Applied {
    /// The last document, reopened, with the camera saved in it; `None` if there was none or it
    /// didn't open.
    pub opened: Option<crate::files::Opened>,
    /// Whether the status bar shows performance; a UI flag the session only carries.
    pub show_performance: bool,
}

pub struct Session {
    pub(crate) catalog: Catalog,
    pub(crate) strings: Strings,
    pub(crate) doc: Document,
    pub(crate) files: Files,
    /// The data folder and what was loaded from it; `None` when the session was given a catalog.
    data: Option<(DataFolder, DataChoice)>,
    selection: Option<Selection>,
    /// Nodes copied with Ctrl+C, and how many times they were pasted since.
    clipboard: Option<(Graph, u32)>,
    availability: Availability,
    /// The last refused action or file message, shown until the next one.
    pub(crate) status: Option<String>,
    /// The data version of the open file, when it isn't the active one.
    pub(crate) other_data_version: Option<String>,
    /// Show whole facilities and how busy they are instead of fractional counts.
    round_up: bool,
    last_solve: Option<Duration>,
}

impl Session {
    /// A session on an empty graph in the catalog's first region.
    pub fn new(catalog: Catalog, availability: Availability) -> Self {
        let region = catalog
            .regions()
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| RegionId::from(""));
        Self {
            catalog,
            strings: Strings::english(),
            doc: Document::new(region),
            files: Files::new(),
            data: None,
            selection: None,
            clipboard: None,
            availability,
            status: None,
            other_data_version: None,
            round_up: false,
            last_solve: None,
        }
    }

    // Reading

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn strings(&self) -> &Strings {
        &self.strings
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn availability(&self) -> &Availability {
        &self.availability
    }

    pub fn availability_mut(&mut self) -> &mut Availability {
        &mut self.availability
    }

    /// The catalog, the availability to edit and the data folder with its choice, borrowed apart
    /// for a UI that shows them in one settings window.
    pub fn settings_parts(
        &mut self,
    ) -> (
        &Catalog,
        &mut Availability,
        Option<(&DataFolder, &DataChoice)>,
    ) {
        (
            &self.catalog,
            &mut self.availability,
            self.data.as_ref().map(|(folder, choice)| (folder, choice)),
        )
    }

    pub fn round_up(&self) -> bool {
        self.round_up
    }

    pub fn set_round_up(&mut self, on: bool) {
        self.round_up = on;
    }

    /// The data version of the open file, when it isn't the active one.
    pub fn other_data_version(&self) -> Option<&str> {
        self.other_data_version.as_deref()
    }

    pub fn set_other_data_version(&mut self, version: Option<String>) {
        self.other_data_version = version;
    }

    /// How long the last solve took.
    pub fn last_solve(&self) -> Option<Duration> {
        self.last_solve
    }

    /// Shows `status` until the next one; `None` clears it.
    pub fn set_status(&mut self, status: Option<String>) {
        self.status = status;
    }

    /// Says in the status bar that the chosen data didn't load: a data switch that failed, or a
    /// saved choice [`DataFolder::open`] replaced. At startup, call it after the last document
    /// reopens, which sets or clears the status.
    pub fn report_data_failure(&mut self, error: &aic_data::LoadError) {
        self.status = Some(
            self.strings
                .format("data-switch-failed", &[("error", &error.to_string())]),
        );
    }

    /// Whether anything is copied.
    pub fn has_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }

    /// Whether a command can do anything right now; a UI greys out what can't.
    pub fn can_run(&self, command: Command) -> bool {
        match command {
            Command::Undo => self.doc.can_undo(),
            Command::Redo => self.doc.can_redo(),
            Command::Cut | Command::Copy | Command::Duplicate | Command::MakeWorkspace => self
                .selection
                .as_ref()
                .is_some_and(|s| !s.items().is_empty()),
            Command::Paste => self.clipboard.is_some(),
            Command::Delete => self.selection.is_some(),
            Command::Dissolve => self
                .selection
                .as_ref()
                .and_then(Selection::single_workspace)
                .is_some(),
            _ => true,
        }
    }

    // The document

    /// Starts an empty graph in `region`, dropping the open one.
    pub fn new_graph(&mut self, region: RegionId) {
        self.replace_document(Document::new(region));
        self.other_data_version = None;
    }

    /// Shows another document; the selection belonged to the old one.
    pub fn replace_document(&mut self, doc: Document) {
        self.doc = doc;
        self.selection = None;
    }

    /// Ends the current undo step.
    pub fn checkpoint(&mut self) {
        self.doc.checkpoint();
    }

    /// Records that the current state was saved to `path`.
    pub fn mark_saved(&mut self, path: std::path::PathBuf, draft: bool) {
        self.doc.mark_saved(path, draft);
    }

    /// Opened from a draft file: keep saving there.
    pub fn set_draft(&mut self, draft: bool) {
        self.doc.set_draft(draft);
    }

    /// Re-solves if anything changed since the last solve, and says how long it took.
    pub fn solve_if_stale(&mut self) -> Option<Duration> {
        let took = self.doc.solve_if_stale(&self.catalog);
        if took.is_some() {
            self.last_solve = took;
        }
        took
    }

    /// What a UI keeps between runs.
    pub fn settings(&self, show_performance: bool) -> Settings {
        Settings {
            data: self.data_choice().cloned().unwrap_or_default(),
            availability: self.availability.clone(),
            recent_files: self.recent_files().to_vec(),
            last_document: self.last_document().map(Path::to_owned),
            show_performance,
        }
    }

    /// Applies settings read from a UI's settings file: the data choice (when there is a data
    /// folder and the choice differs), the raw resource availability and the recent files, then
    /// reopens the last document. A choice that doesn't load keeps the current data and says why
    /// in [`Self::status`]; a last document that is gone says so too, and the session carries on
    /// with its new draft. When both go wrong, the data error is the one left in the status.
    pub fn apply_settings(&mut self, settings: &Settings) -> Applied {
        if self
            .data_choice()
            .is_some_and(|current| *current != settings.data)
        {
            self.switch_data(settings.data.clone());
        }
        // Kept apart: a reopen sets or clears the status, and must not hide this one.
        let data_error = self.status.take();
        self.availability = settings.availability.clone();
        self.set_recent_files(settings.recent_files.clone());
        let opened = settings
            .last_document
            .as_deref()
            .and_then(|path| self.reopen(path));
        if data_error.is_some() {
            self.status = data_error;
        }
        Applied {
            opened,
            show_performance: settings.show_performance,
        }
    }

    /// The data folder the catalog came from, and the version and language chosen in it.
    pub fn set_data_folder(&mut self, folder: DataFolder, choice: DataChoice) {
        self.data = Some((folder, choice));
    }

    pub fn data_folder(&self) -> Option<&DataFolder> {
        self.data.as_ref().map(|(folder, _)| folder)
    }

    pub fn data_choice(&self) -> Option<&DataChoice> {
        self.data.as_ref().map(|(_, choice)| choice)
    }

    /// Loads another data version or language from the data folder. The open graph stays; its
    /// recipe nodes are matched against the new data by ID. A failure keeps the current data and
    /// says why in [`Self::status`]. Returns whether the data changed.
    pub fn switch_data(&mut self, choice: DataChoice) -> bool {
        let Some((folder, _)) = &self.data else {
            return false;
        };
        match folder.load(&choice) {
            Ok(catalog) => {
                self.switch_catalog(catalog);
                if let Some((_, current)) = &mut self.data {
                    *current = choice;
                }
                true
            }
            Err(error) => {
                self.report_data_failure(&error);
                false
            }
        }
    }

    /// Switches to a catalog of another data version or language: the graph's recipe nodes are
    /// matched against it by ID. Not an undo step.
    pub fn switch_catalog(&mut self, catalog: Catalog) {
        self.doc.rebind(&catalog);
        self.catalog = catalog;
        self.status = None;
    }

    /// Moves the graph to another region.
    pub fn set_region(&mut self, region: RegionId) {
        self.doc.set_region(region);
    }

    // Selection

    pub fn select(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    /// Selects these nodes, as a box selection would.
    pub fn select_nodes(&mut self, nodes: Vec<NodeId>) {
        self.select(Selection::of_nodes(nodes));
    }

    /// Adds `member` to the selection, or takes it out if it is in.
    pub fn toggle(&mut self, member: Member) {
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

    /// Selects everything at `level`.
    pub fn select_all(&mut self, level: Level) {
        let level = self.doc.resolve_level(level);
        let all = self.doc.graph().members_of(level);
        self.select(Selection::of_items(all));
    }

    // Edits

    /// Adds a node to `level` at `pos`, without selecting it.
    pub fn add_node(&mut self, kind: NodeKind, pos: Pos, level: Level) -> NodeId {
        self.doc.add_node(kind, pos, level)
    }

    /// Moves a node or workspace.
    pub fn move_member(&mut self, member: Member, pos: Pos) {
        self.doc.move_member(member, pos);
    }

    /// Removes a node, and takes it out of the selection.
    pub fn remove_node(&mut self, node: NodeId) {
        self.doc.remove_node(node);
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

    /// Removes a workspace with everything in it, and clears the selection.
    pub fn remove_workspace(&mut self, workspace: WorkspaceId) {
        self.doc.remove_workspace(workspace);
        self.select(None);
    }

    /// Links two ports, leaving the refusal to the caller.
    pub fn try_connect(&mut self, from: PortRef, to: PortRef) -> Result<LinkId, LinkError> {
        self.doc.connect(&self.catalog, from, to)
    }

    /// Links two ports. A refusal sets [`Self::status`] to the reason.
    pub fn connect(&mut self, from: PortRef, to: PortRef) {
        match self.try_connect(from, to) {
            Ok(_) => self.status = None,
            Err(error) => {
                let reason = link_error(&self.catalog, &self.strings, &error);
                self.status = Some(self.strings.format("link-refused", &[("error", &reason)]));
            }
        }
    }

    /// Removes a link, and the selection if it was the link.
    pub fn disconnect(&mut self, link: LinkId) {
        self.doc.remove_link(link);
        if self.selection == Some(Selection::Link(link)) {
            self.select(None);
        }
    }

    /// Removes every link at `port`.
    pub fn clear_port(&mut self, port: &Port) {
        self.doc.clear_port(port);
    }

    pub fn set_limit(&mut self, node: NodeId, limit: Option<f64>) -> Result<(), EditError> {
        self.doc.set_limit(node, limit)
    }

    pub fn set_port_limit(&mut self, port: &Port, rate: Option<f64>) -> Result<(), EditError> {
        self.doc.set_port_limit(&self.catalog, port, rate)
    }

    pub fn set_share(
        &mut self,
        link: LinkId,
        side: Side,
        share: Option<f64>,
    ) -> Result<(), EditError> {
        self.doc.set_share(link, side, share)
    }

    pub fn set_facility(&mut self, node: NodeId, facility: FacilityId) -> Result<(), EditError> {
        self.doc.set_facility(node, facility)
    }

    pub fn rename_workspace(
        &mut self,
        workspace: WorkspaceId,
        name: &str,
    ) -> Result<(), EditError> {
        self.doc.rename_workspace(workspace, name)
    }

    /// Adds a port to the workspace `level`, and selects it. At the top there is none to add.
    pub fn add_port(&mut self, level: Level, side: Side) -> Option<NodeId> {
        let port = self.doc.add_port(level, side)?;
        self.select(Some(Selection::node(port)));
        Some(port)
    }

    /// Adds an empty workspace to `level` at `pos`, named by number, and selects it.
    pub fn add_workspace(&mut self, pos: Pos, level: Level) -> WorkspaceId {
        let name = self.new_workspace_name();
        let workspace = self.doc.add_workspace(&name, level, pos);
        self.select(Selection::of_items(vec![Member::Workspace(workspace)]));
        workspace
    }

    /// Adds the node the player picked and, when the picker was opened from a port, links it
    /// there. One click does all of it. `at` is where the click was, in graph space: the new node
    /// goes there, and the returned anchor says where its edge should land once the UI has
    /// measured it.
    pub fn add_from_picker(
        &mut self,
        origin: &Origin,
        at: Pos,
        choice: Choice,
        level: Level,
    ) -> Option<(NodeId, Option<Anchor>)> {
        let item = match origin {
            Origin::Port { item, .. } => Some(item),
            Origin::Canvas => None,
        };
        let kind = match choice {
            Choice::Recipe(recipe) => picker::recipe_node(&self.catalog, recipe)?,
            Choice::Builtin(builtin) => picker::builtin_kind(&self.catalog, builtin, item),
            Choice::Workspace => return None,
        };
        // Roughly line the new node's first port up with the point.
        let pos = Pos {
            x: at.x,
            y: at.y - 40.0,
        };
        let node = self.doc.add_node(kind, pos, level);
        let anchor = match origin {
            Origin::Canvas => None,
            Origin::Port { side, .. } => Some(match side {
                Side::Input => Anchor::Right(at.x),
                Side::Output => Anchor::Left(at.x),
            }),
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
        Some((node, anchor))
    }

    // Undo

    /// Goes back one undo step.
    pub fn undo(&mut self) -> bool {
        let undone = self.doc.undo();
        if undone {
            self.forget_missing_selection();
        }
        undone
    }

    /// Goes forward one undone step.
    pub fn redo(&mut self) -> bool {
        let redone = self.doc.redo();
        if redone {
            self.forget_missing_selection();
        }
        redone
    }

    /// Drops selected nodes or a link that an undo or redo took away.
    fn forget_missing_selection(&mut self) {
        let graph = self.doc.graph();
        let selection = match self.selection.take() {
            Some(Selection::Link(link)) if graph.link(link).is_some() => {
                Some(Selection::Link(link))
            }
            Some(Selection::Items(items)) => Selection::of_items(
                items
                    .into_iter()
                    .filter(|item| match *item {
                        Member::Node(id) => graph.node(id).is_some(),
                        Member::Workspace(id) => graph.workspace_info(id).is_some(),
                    })
                    .collect(),
            ),
            _ => None,
        };
        self.selection = selection;
    }

    // Clipboard

    fn selected_items(&self) -> Vec<Member> {
        self.selection
            .iter()
            .flat_map(|s| s.items())
            .copied()
            .collect()
    }

    /// Copies the selection: nodes, and workspaces with everything in them.
    /// Returns whether anything was selected to copy.
    pub fn copy(&mut self) -> bool {
        let items = self.selected_items();
        if items.is_empty() {
            return false;
        }
        self.clipboard = Some((self.doc.graph().copy_members(&items), 0));
        true
    }

    /// The copied nodes as a save file, for the system clipboard.
    pub fn copy_text(&self) -> Option<String> {
        let (graph, _) = self.clipboard.as_ref()?;
        Some(graph.to_json(self.catalog.version()))
    }

    /// Pastes `text` from the system clipboard, from this window or another one. Text that isn't
    /// a copied graph pastes what this window copied last, if anything.
    pub fn paste_text(&mut self, text: &str, into: Level) {
        if let Ok(loaded) = Graph::from_json(&self.catalog, text) {
            let same = self
                .clipboard
                .as_ref()
                .is_some_and(|(graph, _)| graph.to_json("") == loaded.graph.to_json(""));
            if !same {
                self.clipboard = Some((loaded.graph, 0));
            }
        }
        self.paste(into);
    }

    /// Pastes what was copied into `into`, each paste a step further from the originals. Selects
    /// the pasted items when they land at `level`, the level shown.
    pub fn paste(&mut self, level: Level) {
        self.paste_into(level, level);
    }

    /// Pastes what was copied into `into`: the level shown, or a workspace in it.
    pub fn paste_into(&mut self, into: Level, level: Level) {
        let (into, level) = (self.doc.resolve_level(into), self.doc.resolve_level(level));
        let Some((copied, times)) = &mut self.clipboard else {
            return;
        };
        *times += 1;
        let shift = PASTE_OFFSET * *times as f32;
        let pasted = self
            .doc
            .paste(&self.catalog, copied, Pos { x: shift, y: shift }, into);
        if into == level {
            self.select(Selection::of_items(pasted));
        }
    }

    /// Copies the selection a step away from itself and selects the copy.
    pub fn duplicate(&mut self, level: Level) {
        let level = self.doc.resolve_level(level);
        let items = self.selected_items();
        if items.is_empty() {
            return;
        }
        let copy = self.doc.graph().copy_members(&items);
        let pasted = self.doc.paste(
            &self.catalog,
            &copy,
            Pos {
                x: PASTE_OFFSET,
                y: PASTE_OFFSET,
            },
            level,
        );
        self.select(Selection::of_items(pasted));
    }

    pub fn delete_selection(&mut self) {
        match self.selection.take() {
            Some(Selection::Items(items)) => {
                for item in items {
                    match item {
                        Member::Node(id) => self.doc.remove_node(id),
                        Member::Workspace(id) => self.doc.remove_workspace(id),
                    }
                }
            }
            Some(Selection::Link(link)) => self.doc.remove_link(link),
            None => {}
        }
    }

    // Workspaces

    /// "Workspace 3", numbered past the workspaces there are.
    pub fn new_workspace_name(&self) -> String {
        let number = self.doc.graph().workspaces().len() + 1;
        self.strings
            .format("workspace-default-name", &[("number", &number.to_string())])
    }

    /// Puts the selected nodes and workspaces in a new workspace, and selects it. A selection
    /// that can't be one sets [`Self::status`].
    pub fn make_workspace(&mut self) {
        let items = self.selected_items();
        if items.is_empty() {
            return;
        }
        let name = self.new_workspace_name();
        match self.doc.workspace_from(&items, &name) {
            Ok(workspace) => {
                self.select(Selection::of_items(vec![Member::Workspace(workspace)]));
                self.status = None;
            }
            Err(_) => self.status = Some(self.strings.get("workspace-refused")),
        }
    }

    /// Takes the selected workspace apart, selecting what was in it that is at `level`.
    pub fn dissolve_selection(&mut self, level: Level) {
        let Some(workspace) = self
            .selection
            .as_ref()
            .and_then(Selection::single_workspace)
        else {
            return;
        };
        let inside: Vec<Member> = self.doc.graph().members_of(Some(workspace));
        if self.doc.dissolve(workspace).is_ok() {
            let level = self.doc.resolve_level(level);
            let shown = self.doc.graph().members_of(level);
            let kept: Vec<Member> = inside.into_iter().filter(|m| shown.contains(m)).collect();
            self.select(Selection::of_items(kept));
        }
    }

    // Commands

    /// Does what `command` says at `level`, or says what the UI must do to finish it.
    pub fn run(&mut self, command: Command, level: Level) -> Outcome {
        match command {
            Command::New => Outcome::File(FileCommand::New),
            Command::Open => Outcome::File(FileCommand::Open),
            Command::Save => Outcome::File(FileCommand::Save),
            Command::SaveAs => Outcome::File(FileCommand::SaveAs),
            Command::Undo => Self::restored(self.undo()),
            Command::Redo => Self::restored(self.redo()),
            Command::Cut => {
                let copied = self.copy();
                self.delete_selection();
                self.written(copied)
            }
            Command::Copy => {
                let copied = self.copy();
                self.written(copied)
            }
            Command::Paste => Outcome::ReadClipboard,
            Command::Duplicate => {
                self.duplicate(level);
                Outcome::Done
            }
            Command::Delete => {
                self.delete_selection();
                Outcome::Done
            }
            Command::SelectAll => {
                self.select_all(level);
                Outcome::Done
            }
            Command::MakeWorkspace => {
                self.make_workspace();
                Outcome::Done
            }
            Command::Dissolve => {
                self.dissolve_selection(level);
                Outcome::Done
            }
            Command::SplitRight => Outcome::View(ViewCommand::SplitRight),
            Command::SplitDown => Outcome::View(ViewCommand::SplitDown),
            Command::DuplicateView => Outcome::View(ViewCommand::Duplicate),
            Command::MoveToWindow => Outcome::View(ViewCommand::MoveToWindow),
            Command::CloseView => Outcome::View(ViewCommand::Close),
        }
    }

    /// Text for the system clipboard, only if this command copied something: a Copy with nothing
    /// selected must not overwrite what the player has on the clipboard.
    fn written(&self, copied: bool) -> Outcome {
        self.copy_text()
            .filter(|_| copied)
            .map_or(Outcome::Nothing, Outcome::WriteClipboard)
    }

    fn restored(did: bool) -> Outcome {
        if did {
            Outcome::Restored
        } else {
            Outcome::Nothing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::Builtin;
    use crate::testing::fixture_catalog;

    fn session() -> Session {
        Session::new(fixture_catalog(), Availability::default())
    }

    fn smelter() -> NodeKind {
        NodeKind::Recipe {
            recipe: "iron_ingot".into(),
            facility: "smelter".into(),
            limit: Some(1.0),
        }
    }

    fn origin() -> Pos {
        Pos::default()
    }

    fn position(session: &Session, node: NodeId) -> Pos {
        session.document().graph().node(node).unwrap().pos
    }

    #[test]
    fn each_paste_lands_a_step_further_from_the_copy() {
        let mut session = session();
        let node = session.add_node(smelter(), origin(), None);
        session.select(Some(Selection::node(node)));
        session.copy();
        session.paste(None);
        let first = session.selection().unwrap().nodes()[0];
        session.paste(None);
        let second = session.selection().unwrap().nodes()[0];
        assert_eq!(position(&session, first), Pos { x: 40.0, y: 40.0 });
        assert_eq!(position(&session, second), Pos { x: 80.0, y: 80.0 });
    }

    #[test]
    fn duplicate_selects_the_copy() {
        let mut session = session();
        let node = session.add_node(smelter(), origin(), None);
        session.select(Some(Selection::node(node)));
        session.duplicate(None);
        let copy = session.selection().unwrap().single_node().unwrap();
        assert_ne!(copy, node);
        assert_eq!(session.document().graph().nodes().len(), 2);
    }

    #[test]
    fn deleting_a_selected_link_removes_only_the_link() {
        let mut session = session();
        let from = session.add_node(smelter(), origin(), None);
        let to = session.add_node(NodeKind::Splitter { item: None }, origin(), None);
        session.connect(PortRef::new(from, "iron_ingot"), PortRef::any(to));
        let link = session.document().graph().links().keys().next().unwrap();
        session.select(Some(Selection::Link(link)));
        assert_eq!(session.run(Command::Delete, None), Outcome::Done);
        assert!(session.document().graph().links().is_empty());
        assert_eq!(session.document().graph().nodes().len(), 2);
        assert!(session.selection().is_none());
    }

    #[test]
    fn a_workspace_needs_a_selection_that_can_be_one() {
        let mut session = session();
        // Nothing selected: nothing happens.
        session.make_workspace();
        assert!(session.status().is_none());
        assert!(session.document().graph().workspaces().is_empty());

        // A node at the top and one inside a workspace can't go in one together.
        let outside = session.add_node(smelter(), origin(), None);
        let workspace = session.add_workspace(origin(), None);
        let inside = session.add_node(smelter(), origin(), Some(workspace));
        session.select(Selection::of_items(vec![
            Member::Node(outside),
            Member::Node(inside),
        ]));
        session.make_workspace();
        assert_eq!(
            session.status(),
            Some(session.strings().get("workspace-refused").as_str())
        );
        assert_eq!(session.document().graph().workspaces().len(), 1);
    }

    #[test]
    fn making_a_workspace_selects_it_and_dissolving_selects_what_was_inside() {
        let mut session = session();
        let a = session.add_node(smelter(), origin(), None);
        let b = session.add_node(smelter(), origin(), None);
        session.select_nodes(vec![a, b]);
        session.make_workspace();
        let workspace = session.selection().unwrap().single_workspace().unwrap();
        assert_eq!(session.document().graph().members_of(None).len(), 1);

        session.dissolve_selection(None);
        assert!(session.document().graph().workspaces().is_empty());
        let mut selected = session.selection().unwrap().nodes();
        selected.sort();
        let mut expected = vec![a, b];
        expected.sort();
        assert_eq!(selected, expected);
        let _ = workspace;
    }

    #[test]
    fn a_refused_link_sets_the_status_and_a_good_one_clears_it() {
        let mut session = session();
        let ore = session.add_node(
            NodeKind::Recipe {
                recipe: "iron_ore".into(),
                facility: "miner".into(),
                limit: None,
            },
            origin(),
            None,
        );
        let ingot = session.add_node(smelter(), origin(), None);
        // The ore recipe makes ore, which the second node's output isn't.
        session.connect(
            PortRef::new(ore, "iron_ore"),
            PortRef::new(ingot, "iron_ingot"),
        );
        assert!(session.status().is_some_and(|s| !s.is_empty()));
        assert!(session.document().graph().links().is_empty());

        session.connect(
            PortRef::new(ore, "iron_ore"),
            PortRef::new(ingot, "iron_ore"),
        );
        assert!(session.status().is_none());
        assert_eq!(session.document().graph().links().len(), 1);
    }

    #[test]
    fn picking_from_an_input_port_adds_and_links_a_producer_anchored_on_its_right() {
        let mut session = session();
        let consumer = session.add_node(smelter(), origin(), None);
        let origin_port = Origin::Port {
            node: consumer,
            side: Side::Input,
            item: "iron_ore".into(),
        };
        let (producer, anchor) = session
            .add_from_picker(
                &origin_port,
                Pos { x: 200.0, y: 100.0 },
                Choice::Builtin(Builtin::Source),
                None,
            )
            .unwrap();
        assert_eq!(anchor, Some(Anchor::Right(200.0)));
        let graph = session.document().graph();
        let link = graph.links().values().next().unwrap();
        assert_eq!((link.from, link.to), (producer, consumer));
        assert_eq!(graph.node(producer).unwrap().pos, Pos { x: 200.0, y: 60.0 });
        assert_eq!(session.selection(), Some(&Selection::node(producer)));
    }

    #[test]
    fn copy_text_pastes_into_another_session() {
        let mut one = session();
        let node = one.add_node(smelter(), origin(), None);
        one.select(Some(Selection::node(node)));
        one.copy();
        let text = one.copy_text().expect("something is copied");

        let mut other = session();
        other.paste_text(&text, None);
        assert_eq!(other.document().graph().nodes().len(), 1);
        assert_eq!(other.selection().unwrap().nodes().len(), 1);
    }

    #[test]
    fn view_and_file_commands_are_the_uis_to_finish() {
        let mut session = session();
        assert_eq!(
            session.run(Command::Save, None),
            Outcome::File(FileCommand::Save)
        );
        assert_eq!(
            session.run(Command::SplitRight, None),
            Outcome::View(ViewCommand::SplitRight)
        );
    }

    #[test]
    fn a_view_naming_a_workspace_an_undo_took_away_pastes_to_the_top() {
        let mut session = session();
        let node = session.add_node(smelter(), origin(), None);
        session.select(Some(Selection::node(node)));
        session.copy();
        session.checkpoint();
        let ws = session.add_workspace(origin(), None);
        session.checkpoint();
        session.undo();
        session.paste(Some(ws));
        let graph = session.document().graph();
        assert!(graph.nodes().values().all(|n| n.workspace.is_none()));
        assert_eq!(graph.nodes().len(), 2);
    }

    #[test]
    fn copy_writes_the_clipboard_only_when_something_was_copied() {
        let mut session = session();
        let node = session.add_node(smelter(), origin(), None);
        session.select(Some(Selection::node(node)));
        assert!(matches!(
            session.run(Command::Copy, None),
            Outcome::WriteClipboard(_)
        ));
        // Nothing selected: the earlier copy must not go to the system clipboard again.
        session.select(None);
        assert_eq!(session.run(Command::Copy, None), Outcome::Nothing);
        assert_eq!(session.run(Command::Cut, None), Outcome::Nothing);
    }

    #[test]
    fn paste_asks_the_ui_for_the_system_clipboard() {
        let mut session = session();
        let node = session.add_node(smelter(), origin(), None);
        session.select(Some(Selection::node(node)));
        session.copy();
        assert_eq!(session.run(Command::Paste, None), Outcome::ReadClipboard);
        assert_eq!(
            session.document().graph().nodes().len(),
            1,
            "nothing pasted yet"
        );
    }

    #[test]
    fn undo_and_redo_say_the_graph_was_replaced() {
        let mut session = session();
        assert_eq!(session.run(Command::Undo, None), Outcome::Nothing);
        session.add_node(smelter(), origin(), None);
        session.checkpoint();
        assert_eq!(session.run(Command::Undo, None), Outcome::Restored);
        assert_eq!(session.run(Command::Redo, None), Outcome::Restored);
        assert_eq!(session.run(Command::Redo, None), Outcome::Nothing);
    }

    #[test]
    fn applying_settings_restores_availability_recent_files_and_the_last_document() {
        let mut one = session();
        let dir = std::env::temp_dir().join(format!("aic-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        one.add_node(smelter(), origin(), None);
        let file = dir.join("plan.aicm");
        one.save_to(&file, web_time::Instant::now(), None);
        let mut settings = one.settings(false);
        settings
            .availability
            .set(&"tundra".into(), &"iron_ore".into(), Some(120.0));

        let mut other = session();
        let applied = other.apply_settings(&settings);
        assert!(!applied.show_performance);
        assert!(applied.opened.is_some());
        assert_eq!(other.document().graph().nodes().len(), 1);
        assert_eq!(other.recent_files(), std::slice::from_ref(&file));
        assert_eq!(
            other
                .availability()
                .get(&"tundra".into(), &"iron_ore".into()),
            Some(120.0)
        );

        // A last document that is gone says so, and the session carries on.
        let mut gone = settings.clone();
        gone.last_document = Some(dir.join("gone.aicm"));
        let mut third = session();
        assert!(third.apply_settings(&gone).opened.is_none());
        assert!(third.status().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_data_choice_that_does_not_load_stays_in_the_status_after_the_last_document_opens() {
        let dir = std::env::temp_dir().join(format!("aic-apply-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.json"), "not json").unwrap();
        let folder = DataFolder::at(dir.clone()).unwrap();

        let mut one = session();
        one.add_node(smelter(), origin(), None);
        let file = dir.join("plan.aicm");
        one.save_to(&file, web_time::Instant::now(), None);

        let mut settings = one.settings(false);
        settings.data = DataChoice {
            version: Some("broken".into()),
            language: None,
        };
        settings.last_document = Some(file);
        let mut other = session();
        other.set_data_folder(folder, DataChoice::default());
        assert!(other.apply_settings(&settings).opened.is_some());
        assert!(other.status().is_some(), "the data error is still shown");
        let _ = std::fs::remove_dir_all(dir);
    }
}
