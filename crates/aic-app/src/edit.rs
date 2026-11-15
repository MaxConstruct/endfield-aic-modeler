//! The Edit menu and keyboard shortcuts. What a command does is [`aic_editor::Session`]'s; this
//! maps the keys to egui's and finishes what the session leaves to the toolkit.

use aic_core::WorkspaceId;
use aic_editor::{Chord, ChordKey, Command, FileCommand, Outcome, ViewCommand};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use crate::editor::Editor;
use crate::files::Pending;
use crate::views::ViewAction;

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
        ChordKey::Letter(letter) => letter_key(letter),
        ChordKey::Delete => Key::Delete,
        ChordKey::Backspace => Key::Backspace,
        ChordKey::Backslash => Key::Backslash,
    };
    KeyboardShortcut::new(modifiers, key)
}

/// The key of a letter, upper case.
fn letter_key(letter: char) -> Key {
    const LETTERS: [Key; 26] = [
        Key::A,
        Key::B,
        Key::C,
        Key::D,
        Key::E,
        Key::F,
        Key::G,
        Key::H,
        Key::I,
        Key::J,
        Key::K,
        Key::L,
        Key::M,
        Key::N,
        Key::O,
        Key::P,
        Key::Q,
        Key::R,
        Key::S,
        Key::T,
        Key::U,
        Key::V,
        Key::W,
        Key::X,
        Key::Y,
        Key::Z,
    ];
    let at = (letter as usize).wrapping_sub('A' as usize);
    *LETTERS.get(at).expect("a chord letter is A to Z")
}

/// The keys that do `command`.
pub fn keys(command: Command) -> KeyboardShortcut {
    shortcut_of(command.chord())
}

impl Editor {
    pub(crate) fn keyboard(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused().is_some());
        let overlay = self.picker.is_some() || self.port_menu.is_some();
        if overlay {
            return;
        }
        for command in Command::ALL {
            if typing && !command.works_while_typing() {
                continue;
            }
            // egui reads Ctrl as Command off macOS; Backspace deletes too, as on a Mac keyboard.
            let pressed = ctx.input_mut(|i| i.consume_shortcut(&keys(command)))
                || command.also().is_some_and(|chord| {
                    ctx.input_mut(|i| i.consume_shortcut(&shortcut_of(chord)))
                });
            if pressed {
                self.flash(ctx, command);
                self.run(command);
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
            self.flash(ctx, if cut { Command::Cut } else { Command::Copy });
            self.session.copy();
            if let Some(text) = self.session.copy_text() {
                ctx.copy_text(text);
            }
        }
        if cut {
            self.session.delete_selection();
        }
        if let Some(text) = pasted {
            self.flash(ctx, Command::Paste);
            self.session.paste_text(&text, self.view.level());
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.select(None);
        }
    }

    pub fn run(&mut self, command: Command) {
        let focused = self.views.focused();
        match command {
            // A menu or the picker pointed at what an undo or redo may take away.
            Command::Undo => return self.undo(),
            Command::Redo => return self.redo(),
            _ => {}
        }
        match self.session.run(command, self.view.level()) {
            Outcome::File(FileCommand::New) => self.request(Pending::New),
            Outcome::File(FileCommand::Open) => self.request(Pending::Open),
            Outcome::File(FileCommand::Save) => self.save(),
            Outcome::File(FileCommand::SaveAs) => self.save_as(),
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
            // egui pastes on its own Paste event; a menu click pastes what this window copied.
            Outcome::ReadClipboard => self.session.paste(self.view.level()),
            // Only comes when something was copied. A key press puts it on the system clipboard
            // (see `keyboard`); a menu click copies within the app only.
            Outcome::WriteClipboard(_) | Outcome::Restored | Outcome::Done | Outcome::Nothing => {}
        }
    }

    /// Lights the shortcut's toolbar button for a moment, so a key press shows where its button is.
    /// The time is egui's, so a test's stepped clock decides when it goes dark.
    pub(crate) fn flash(&mut self, ctx: &egui::Context, shortcut: Command) {
        // The toolbar is drawn before the keys are read; egui draws another frame after any input
        // event, and that one lights it.
        self.flash = Some((shortcut, ctx.input(|i| i.time)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_chord_maps_to_a_key() {
        for command in Command::ALL {
            let _ = keys(command);
            if let Some(chord) = command.also() {
                let _ = shortcut_of(chord);
            }
        }
        assert_eq!(letter_key('Z'), Key::Z);
        assert_eq!(keys(Command::Redo).logical_key, Key::Z);
    }
}
