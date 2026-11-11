//! The Edit menu and keyboard shortcuts: undo and redo, cut, copy and paste within the app,
//! duplicate, delete, select all, and making or dissolving a workspace.

use aic_core::{Member, Pos, WorkspaceId};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use crate::canvas::Selection;
use crate::editor::Editor;
use crate::files::Pending;
use crate::i18n::Strings;

/// How far each paste lands from the copied nodes, in graph units.
const PASTE_OFFSET: f32 = 40.0;

/// Something the player can do from a menu or with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    New,
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    MakeWorkspace,
    Dissolve,
}

impl Shortcut {
    /// Checked in this order, so Cmd+Shift+S is Save As and not Save.
    const ALL: [Self; 14] = [
        Self::Dissolve,
        Self::MakeWorkspace,
        Self::SaveAs,
        Self::Save,
        Self::New,
        Self::Open,
        Self::Redo,
        Self::Undo,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::Duplicate,
        Self::Delete,
        Self::SelectAll,
    ];

    pub fn keys(self) -> KeyboardShortcut {
        let command = Modifiers::COMMAND;
        let shift_command = Modifiers::COMMAND | Modifiers::SHIFT;
        match self {
            Self::New => KeyboardShortcut::new(command, Key::N),
            Self::Open => KeyboardShortcut::new(command, Key::O),
            Self::Save => KeyboardShortcut::new(command, Key::S),
            Self::SaveAs => KeyboardShortcut::new(shift_command, Key::S),
            Self::Undo => KeyboardShortcut::new(command, Key::Z),
            Self::Redo => KeyboardShortcut::new(shift_command, Key::Z),
            Self::Cut => KeyboardShortcut::new(command, Key::X),
            Self::Copy => KeyboardShortcut::new(command, Key::C),
            Self::Paste => KeyboardShortcut::new(command, Key::V),
            Self::Duplicate => KeyboardShortcut::new(command, Key::D),
            Self::Delete => KeyboardShortcut::new(Modifiers::NONE, Key::Delete),
            Self::SelectAll => KeyboardShortcut::new(command, Key::A),
            Self::MakeWorkspace => KeyboardShortcut::new(command, Key::G),
            Self::Dissolve => KeyboardShortcut::new(shift_command, Key::G),
        }
    }

    /// Whether the key works while a text field has focus. Text fields handle their own undo,
    /// copy, paste and delete.
    fn works_while_typing(self) -> bool {
        matches!(self, Self::New | Self::Open | Self::Save | Self::SaveAs)
    }
}

impl Editor {
    pub(crate) fn keyboard(&mut self, ctx: &egui::Context, strings: &Strings) {
        let typing = ctx.memory(|m| m.focused().is_some());
        let overlay = self.picker.is_some() || self.port_menu.is_some();
        if overlay {
            return;
        }
        for shortcut in Shortcut::ALL {
            if typing && !shortcut.works_while_typing() {
                continue;
            }
            // egui reads Ctrl as Command off macOS; Backspace deletes too, as on a Mac keyboard.
            let pressed = ctx.input_mut(|i| i.consume_shortcut(&shortcut.keys()))
                || shortcut == Shortcut::Delete
                    && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Backspace));
            if pressed {
                self.flash(shortcut);
                self.run(shortcut, strings);
            }
        }
        if typing {
            return;
        }
        // The window turns Cmd+C, Cmd+X and Cmd+V into clipboard events instead of key presses.
        let (copy, cut, pasted) = ctx.input(|i| {
            let mut pasted = None;
            let (mut copy, mut cut) = (false, false);
            for event in &i.events {
                match event {
                    egui::Event::Copy => copy = true,
                    egui::Event::Cut => cut = true,
                    egui::Event::Paste(text) => pasted = Some(text.clone()),
                    _ => {}
                }
            }
            (copy, cut, pasted)
        });
        if copy || cut {
            self.flash(if cut { Shortcut::Cut } else { Shortcut::Copy });
            self.copy();
            if let Some((graph, _)) = &self.clipboard {
                ctx.copy_text(graph.to_json(self.catalog.version()));
            }
        }
        if cut {
            self.delete_selection();
        }
        if let Some(text) = pasted {
            self.flash(Shortcut::Paste);
            self.paste_text(&text);
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.select(None);
        }
    }

    /// Pastes nodes copied to the system clipboard, from this window or another one. Text that
    /// isn't a copied graph pastes what this window copied last, if anything.
    fn paste_text(&mut self, text: &str) {
        if let Ok(loaded) = aic_core::Graph::from_json(&self.catalog, text) {
            let same = self
                .clipboard
                .as_ref()
                .is_some_and(|(graph, _)| graph.to_json("") == loaded.graph.to_json(""));
            if !same {
                self.clipboard = Some((loaded.graph, 0));
            }
        }
        self.paste();
    }

    pub fn run(&mut self, shortcut: Shortcut, strings: &Strings) {
        match shortcut {
            Shortcut::New => self.request(Pending::New, strings),
            Shortcut::Open => self.request(Pending::Open, strings),
            Shortcut::Save => self.save(strings),
            Shortcut::SaveAs => self.save_as(strings),
            Shortcut::Undo => self.undo(),
            Shortcut::Redo => self.redo(),
            Shortcut::Cut => {
                self.copy();
                self.delete_selection();
            }
            Shortcut::Copy => self.copy(),
            Shortcut::Paste => self.paste(),
            Shortcut::Duplicate => self.duplicate(),
            Shortcut::Delete => self.delete_selection(),
            Shortcut::SelectAll => {
                let all = self.doc.view.nodes().copied().collect();
                self.select(Selection::of_items(all));
            }
            Shortcut::MakeWorkspace => self.make_workspace(strings),
            Shortcut::Dissolve => self.dissolve_selection(),
        }
    }

    /// Lights the shortcut's toolbar button for a moment, so a key press shows where its button is.
    pub(crate) fn flash(&mut self, shortcut: Shortcut) {
        self.flash = Some((shortcut, web_time::Instant::now()));
    }

    pub(crate) fn has_selected_nodes(&self) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|s| !s.items().is_empty())
    }

    pub fn undo(&mut self) {
        if self.doc.undo(&self.catalog) {
            self.forget_missing_selection();
        }
    }

    pub fn redo(&mut self) {
        if self.doc.redo(&self.catalog) {
            self.forget_missing_selection();
        }
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
                    .filter(|item| self.doc.view_id(*item).is_some())
                    .collect(),
            ),
            _ => None,
        };
        self.select(selection);
        self.port_menu = None;
        self.picker = None;
    }

    /// The selected nodes and workspaces.
    fn selected_items(&self) -> Vec<Member> {
        self.selection
            .iter()
            .flat_map(|s| s.items())
            .copied()
            .collect()
    }

    /// Copies the selection: nodes, and workspaces with everything in them.
    pub fn copy(&mut self) {
        let items = self.selected_items();
        if !items.is_empty() {
            self.clipboard = Some((self.doc.graph().copy_members(&items), 0));
        }
    }

    /// Pastes what was copied into the level shown, each paste a step further from the originals,
    /// and selects it.
    pub fn paste(&mut self) {
        let level = self.doc.level();
        self.paste_into(level);
    }

    /// Pastes what was copied into `into`: the level shown, or a workspace in it.
    pub fn paste_into(&mut self, into: Option<WorkspaceId>) {
        let Some((copied, times)) = &mut self.clipboard else {
            return;
        };
        *times += 1;
        let shift = PASTE_OFFSET * *times as f32;
        let pasted = self
            .doc
            .paste(&self.catalog, copied, Pos { x: shift, y: shift }, into);
        if into == self.doc.level() {
            self.select(Selection::of_items(pasted));
        }
    }

    pub fn duplicate(&mut self) {
        let items = self.selected_items();
        if items.is_empty() {
            return;
        }
        let copy = self.doc.graph().copy_members(&items);
        let level = self.doc.level();
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
                        Member::Node(id) => self.doc.remove_node(&self.catalog, id),
                        Member::Workspace(id) => self.doc.remove_workspace(&self.catalog, id),
                    }
                }
            }
            Some(Selection::Link(link)) => self.doc.remove_link(&self.catalog, link),
            None => {}
        }
    }

    /// Puts the selected nodes and workspaces in a new workspace, and selects it.
    pub fn make_workspace(&mut self, strings: &Strings) {
        let items = self.selected_items();
        if items.is_empty() {
            return;
        }
        let name = self.new_workspace_name(strings);
        match self.doc.workspace_from(&self.catalog, &items, &name) {
            Ok(workspace) => {
                self.select(Selection::of_items(vec![Member::Workspace(workspace)]));
                self.status = None;
            }
            Err(_) => self.status = Some(strings.get("workspace-refused")),
        }
    }

    /// "Workspace 3", numbered past the workspaces there are.
    pub(crate) fn new_workspace_name(&self, strings: &Strings) -> String {
        let number = self.doc.graph().workspaces().len() + 1;
        strings.format("workspace-default-name", &[("number", &number.to_string())])
    }

    /// Takes the selected workspace apart, selecting what was in it.
    pub fn dissolve_selection(&mut self) {
        let Some(workspace) = self
            .selection
            .as_ref()
            .and_then(Selection::single_workspace)
        else {
            return;
        };
        let inside: Vec<Member> = self.doc.graph().members_of(Some(workspace));
        if self.doc.dissolve(&self.catalog, workspace).is_ok() {
            let shown: Vec<Member> = inside
                .into_iter()
                .filter(|m| self.doc.view_id(*m).is_some())
                .collect();
            self.select(Selection::of_items(shown));
        }
    }
}
