//! The Edit menu and keyboard shortcuts: undo and redo, copy and paste within the app, duplicate,
//! delete and select all.

use aic_core::{Member, NodeId, Pos};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use crate::canvas::Selection;
use crate::files::Pending;
use crate::i18n::Strings;
use crate::workspace::Workspace;

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
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    Group,
    Ungroup,
}

impl Shortcut {
    /// Checked in this order, so Cmd+Shift+S is Save As and not Save.
    const ALL: [Self; 13] = [
        Self::Ungroup,
        Self::Group,
        Self::SaveAs,
        Self::Save,
        Self::New,
        Self::Open,
        Self::Redo,
        Self::Undo,
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
            Self::Copy => KeyboardShortcut::new(command, Key::C),
            Self::Paste => KeyboardShortcut::new(command, Key::V),
            Self::Duplicate => KeyboardShortcut::new(command, Key::D),
            Self::Delete => KeyboardShortcut::new(Modifiers::NONE, Key::Delete),
            Self::SelectAll => KeyboardShortcut::new(command, Key::A),
            Self::Group => KeyboardShortcut::new(command, Key::G),
            Self::Ungroup => KeyboardShortcut::new(shift_command, Key::G),
        }
    }

    /// Whether the key works while a text field has focus. Text fields handle their own undo,
    /// copy, paste and delete.
    fn works_while_typing(self) -> bool {
        matches!(self, Self::New | Self::Open | Self::Save | Self::SaveAs)
    }
}

impl Workspace {
    pub(crate) fn keyboard(&mut self, ctx: &egui::Context, strings: &Strings) {
        let typing = ctx.memory(|m| m.focused().is_some());
        let overlay = self.picker.is_some()
            || self.port_menu.is_some()
            || self.confirm_discard.is_some()
            || self.new_dialog.is_some();
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
            self.copy();
            if let Some((graph, _)) = &self.clipboard {
                ctx.copy_text(graph.to_json(self.catalog.version()));
            }
        }
        if cut {
            self.delete_selection();
        }
        if let Some(text) = pasted {
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
            Shortcut::Copy => self.copy(),
            Shortcut::Paste => self.paste(),
            Shortcut::Duplicate => self.duplicate(),
            Shortcut::Delete => self.delete_selection(),
            Shortcut::SelectAll => {
                let all = self.doc.view.nodes().copied().collect();
                self.select(Selection::of_items(all));
            }
            Shortcut::Group => self.group_selection(strings),
            Shortcut::Ungroup => self.ungroup_selection(),
        }
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

    /// The selected nodes, with every node inside a selected group.
    fn selected_nodes(&self) -> Vec<NodeId> {
        let graph = self.doc.graph();
        let mut nodes = Vec::new();
        for item in self.selection.iter().flat_map(|s| s.items()) {
            match *item {
                Member::Node(id) => nodes.push(id),
                Member::Group(id) => nodes.extend(graph.nodes_in_group(id)),
            }
        }
        nodes
    }

    pub fn copy(&mut self) {
        let nodes = self.selected_nodes();
        if !nodes.is_empty() {
            self.clipboard = Some((self.doc.graph().copy_nodes(&nodes), 0));
        }
    }

    /// Pastes the copied nodes, each paste a step further from the originals, and selects them.
    pub fn paste(&mut self) {
        let Some((copied, times)) = &mut self.clipboard else {
            return;
        };
        *times += 1;
        let shift = PASTE_OFFSET * *times as f32;
        let pasted = self
            .doc
            .paste(&self.catalog, copied, Pos { x: shift, y: shift });
        self.select(Selection::of_nodes(pasted));
    }

    pub fn duplicate(&mut self) {
        let nodes = self.selected_nodes();
        if nodes.is_empty() {
            return;
        }
        let copy = self.doc.graph().copy_nodes(&nodes);
        let pasted = self.doc.paste(
            &self.catalog,
            &copy,
            Pos {
                x: PASTE_OFFSET,
                y: PASTE_OFFSET,
            },
        );
        self.select(Selection::of_nodes(pasted));
    }

    pub fn delete_selection(&mut self) {
        match self.selection.take() {
            Some(Selection::Items(items)) => {
                for item in items {
                    match item {
                        Member::Node(id) => self.doc.remove_node(&self.catalog, id),
                        Member::Group(id) => self.doc.remove_group(&self.catalog, id),
                    }
                }
            }
            Some(Selection::Link(link)) => self.doc.remove_link(&self.catalog, link),
            None => {}
        }
    }

    /// Puts the selected nodes and groups in a new, collapsed group.
    pub fn group_selection(&mut self, strings: &Strings) {
        let items = self
            .selection
            .as_ref()
            .map(|s| s.items().to_vec())
            .unwrap_or_default();
        if items.is_empty() {
            return;
        }
        let number = self.doc.graph().groups().len() + 1;
        let name = strings.format("group-default-name", &[("number", &number.to_string())]);
        match self.doc.group(&self.catalog, &items, &name) {
            Ok(group) => {
                let _ = self.doc.set_group_collapsed(&self.catalog, group, true);
                self.select(Selection::of_items(vec![Member::Group(group)]));
                self.status = None;
            }
            Err(_) => self.status = Some(strings.get("group-refused")),
        }
    }

    /// Dissolves the selected group, selecting what was in it.
    pub fn ungroup_selection(&mut self) {
        let Some(group) = self.selection.as_ref().and_then(Selection::single_group) else {
            return;
        };
        let graph = self.doc.graph();
        let members: Vec<Member> = graph
            .nodes()
            .iter()
            .filter(|(_, n)| n.group == Some(group))
            .map(|(id, _)| Member::Node(id))
            .chain(
                graph
                    .groups()
                    .iter()
                    .filter(|(_, g)| g.parent == Some(group))
                    .map(|(id, _)| Member::Group(id)),
            )
            .collect();
        if self.doc.ungroup(&self.catalog, group).is_ok() {
            let shown: Vec<Member> = members
                .into_iter()
                .filter(|m| self.doc.view_id(*m).is_some())
                .collect();
            self.select(Selection::of_items(shown));
        }
    }
}
