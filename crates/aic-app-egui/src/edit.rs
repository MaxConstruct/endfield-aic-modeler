//! The Edit menu and keyboard shortcuts. What a command does is [`aic_editor::Session`]'s; this
//! maps the keys to egui's and finishes what the session leaves to the toolkit.

use aic_core::WorkspaceId;
use aic_editor::{Chord, ChordKey, FileCommand, Outcome, ViewCommand};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use crate::editor::Editor;
use crate::files::Pending;
use crate::i18n::Strings;
use crate::views::ViewAction;

/// Something the player can do from a menu or with a key.
pub use aic_editor::Command as Shortcut;

/// A chord as egui's keyboard shortcut. `COMMAND` is Cmd on macOS and Ctrl elsewhere.
fn shortcut_of(chord: Chord) -> KeyboardShortcut {
    let mut modifiers = Modifiers::NONE;
    if chord.command {
        modifiers |= Modifiers::COMMAND;
    }
    if chord.shift {
        modifiers |= Modifiers::SHIFT;
    }
    let key = match chord.key {
        ChordKey::Letter(letter) => Key::from_name(&letter.to_string()).expect("a letter is a key"),
        ChordKey::Delete => Key::Delete,
        ChordKey::Backspace => Key::Backspace,
        ChordKey::Backslash => Key::Backslash,
    };
    KeyboardShortcut::new(modifiers, key)
}

/// The keys that do `command`.
pub fn keys(command: Shortcut) -> KeyboardShortcut {
    shortcut_of(command.chord())
}

impl Editor {
    pub(crate) fn keyboard(&mut self, ctx: &egui::Context, strings: &Strings) {
        let typing = ctx.memory(|m| m.focused().is_some());
        let overlay = self.picker.is_some() || self.port_menu.is_some();
        if overlay {
            return;
        }
        for command in Shortcut::ALL {
            if typing && !command.works_while_typing() {
                continue;
            }
            // egui reads Ctrl as Command off macOS; Backspace deletes too, as on a Mac keyboard.
            let pressed = ctx.input_mut(|i| i.consume_shortcut(&keys(command)))
                || command.also().is_some_and(|chord| {
                    ctx.input_mut(|i| i.consume_shortcut(&shortcut_of(chord)))
                });
            if pressed {
                self.flash(command);
                self.run(command, strings);
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
            self.session.copy();
            if let Some(text) = self.session.copy_text() {
                ctx.copy_text(text);
            }
        }
        if cut {
            self.session.delete_selection();
        }
        if let Some(text) = pasted {
            self.flash(Shortcut::Paste);
            self.session.paste_text(&text, self.view.level());
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.select(None);
        }
    }

    pub fn run(&mut self, command: Shortcut, strings: &Strings) {
        let focused = self.views.focused();
        match command {
            // A menu or the picker pointed at what an undo or redo may take away.
            Shortcut::Undo => return self.undo(),
            Shortcut::Redo => return self.redo(),
            _ => {}
        }
        match self.session.run(command, self.view.level()) {
            Outcome::File(FileCommand::New) => self.request(Pending::New, strings),
            Outcome::File(FileCommand::Open) => self.request(Pending::Open, strings),
            Outcome::File(FileCommand::Save) => self.save(strings),
            Outcome::File(FileCommand::SaveAs) => self.save_as(strings),
            Outcome::View(ViewCommand::SplitRight) => {
                self.views.request(ViewAction::SplitRight(focused));
            }
            Outcome::View(ViewCommand::SplitDown) => {
                self.views.request(ViewAction::SplitDown(focused));
            }
            Outcome::View(ViewCommand::Duplicate) => {
                self.views.request(ViewAction::Duplicate(focused));
            }
            Outcome::View(ViewCommand::MoveToWindow) => {
                self.views.request(ViewAction::MoveToWindow(focused));
            }
            Outcome::View(ViewCommand::Close) => {
                self.views.request(ViewAction::Close(focused));
            }
            // The window puts copied nodes on the system clipboard when the key is pressed (see
            // `keyboard`); a menu click copies within the app only.
            // egui pastes on its own Paste event; a menu click pastes what this window copied.
            Outcome::ReadClipboard => self.session.paste(self.view.level()),
            Outcome::WriteClipboard(_) | Outcome::Restored | Outcome::Done | Outcome::Nothing => {}
        }
    }

    /// Lights the shortcut's toolbar button for a moment, so a key press shows where its button is.
    pub(crate) fn flash(&mut self, shortcut: Shortcut) {
        self.flash = Some((shortcut, web_time::Instant::now()));
    }

    pub fn undo(&mut self) {
        if self.session.undo() {
            self.port_menu = None;
            self.picker = None;
            // The view may name a workspace this took away; later edits in the frame resolve it.
            self.sync_view();
        }
    }

    pub fn redo(&mut self) {
        if self.session.redo() {
            self.port_menu = None;
            self.picker = None;
            // The view may name a workspace this took away; later edits in the frame resolve it.
            self.sync_view();
        }
    }

    /// Copies the selection: nodes, and workspaces with everything in them.
    pub fn copy(&mut self) {
        self.session.copy();
    }

    /// Pastes what was copied into the level shown, each paste a step further from the originals,
    /// and selects it.
    pub fn paste(&mut self) {
        self.session.paste(self.view.level());
    }

    /// Pastes what was copied into `into`: the level shown, or a workspace in it.
    pub fn paste_into(&mut self, into: Option<WorkspaceId>) {
        self.session.paste_into(into, self.view.level());
    }

    pub fn duplicate(&mut self) {
        self.session.duplicate(self.view.level());
    }

    pub fn delete_selection(&mut self) {
        self.session.delete_selection();
    }

    /// Puts the selected nodes and workspaces in a new workspace, and selects it.
    pub fn make_workspace(&mut self) {
        self.session.make_workspace();
    }

    /// Takes the selected workspace apart, selecting what was in it.
    pub fn dissolve_selection(&mut self) {
        self.session.dissolve_selection(self.view.level());
    }
}
