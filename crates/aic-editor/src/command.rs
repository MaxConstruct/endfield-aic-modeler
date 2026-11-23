//! Something the player can do from a menu, the toolbar or a key, and the keys for it, without a
//! toolkit.

/// Something the player can do from a menu, the toolbar or a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
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
    SplitRight,
    SplitDown,
    DuplicateView,
    MoveToWindow,
    CloseView,
}

/// A key that takes part in a [`Chord`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordKey {
    /// A letter key, upper case.
    Letter(char),
    Delete,
    Backspace,
    Backslash,
}

/// A key press with modifiers. `command` is Cmd on macOS and Ctrl elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub command: bool,
    pub shift: bool,
    pub key: ChordKey,
}

impl Chord {
    const fn command(key: ChordKey) -> Self {
        Self {
            command: true,
            shift: false,
            key,
        }
    }

    const fn shift_command(key: ChordKey) -> Self {
        Self {
            command: true,
            shift: true,
            key,
        }
    }
}

impl Command {
    /// Every command. Checked in this order, so Cmd+Shift+S is Save As and not Save: each chord
    /// with Shift comes before the chord of the same key without it.
    pub const ALL: [Self; 19] = [
        Self::SplitDown,
        Self::SplitRight,
        Self::DuplicateView,
        Self::MoveToWindow,
        Self::CloseView,
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

    /// The keys that do it.
    pub fn chord(self) -> Chord {
        use ChordKey::{Backslash, Delete, Letter};
        match self {
            Self::New => Chord::command(Letter('N')),
            Self::Open => Chord::command(Letter('O')),
            Self::Save => Chord::command(Letter('S')),
            Self::SaveAs => Chord::shift_command(Letter('S')),
            Self::Undo => Chord::command(Letter('Z')),
            Self::Redo => Chord::shift_command(Letter('Z')),
            Self::Cut => Chord::command(Letter('X')),
            Self::Copy => Chord::command(Letter('C')),
            Self::Paste => Chord::command(Letter('V')),
            Self::Duplicate => Chord::command(Letter('D')),
            Self::Delete => Chord {
                command: false,
                shift: false,
                key: Delete,
            },
            Self::SelectAll => Chord::command(Letter('A')),
            Self::MakeWorkspace => Chord::command(Letter('G')),
            Self::Dissolve => Chord::shift_command(Letter('G')),
            Self::SplitRight => Chord::command(Backslash),
            Self::SplitDown => Chord::shift_command(Backslash),
            Self::DuplicateView => Chord::shift_command(Letter('D')),
            Self::MoveToWindow => Chord::shift_command(Letter('N')),
            Self::CloseView => Chord::command(Letter('W')),
        }
    }

    /// A second chord that does the same: Backspace deletes too, as on a Mac keyboard.
    pub fn also(self) -> Option<Chord> {
        (self == Self::Delete).then_some(Chord {
            command: false,
            shift: false,
            key: ChordKey::Backspace,
        })
    }

    /// Whether the key works while a text field has focus. Text fields handle their own undo,
    /// copy, paste and delete.
    pub fn works_while_typing(self) -> bool {
        matches!(self, Self::New | Self::Open | Self::Save | Self::SaveAs)
    }

    /// Whether its keys arrive as a clipboard event, which is what does it. A native window sends
    /// only the event; a browser sends the event and the key press, so acting on the key press
    /// as well would do it twice.
    pub fn comes_as_clipboard_event(self) -> bool {
        matches!(self, Self::Cut | Self::Copy | Self::Paste)
    }

    /// The Fluent key of its name, shared by the menus and the toolbar.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::New => "menu-new",
            Self::Open => "menu-open",
            Self::Save => "menu-save",
            Self::SaveAs => "menu-save-as",
            Self::Undo => "menu-undo",
            Self::Redo => "menu-redo",
            Self::Cut => "menu-cut",
            Self::Copy => "menu-copy",
            Self::Paste => "menu-paste",
            Self::Duplicate => "menu-duplicate",
            Self::Delete => "menu-delete",
            Self::SelectAll => "menu-select-all",
            Self::MakeWorkspace => "menu-make-workspace",
            Self::Dissolve => "menu-dissolve-workspace",
            Self::SplitRight => "view-split-right",
            Self::SplitDown => "view-split-down",
            Self::DuplicateView => "view-duplicate",
            Self::MoveToWindow => "view-move-to-window",
            Self::CloseView => "view-close",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shift_chord_is_checked_before_its_plain_twin() {
        for (at, command) in Command::ALL.iter().enumerate() {
            let chord = command.chord();
            if chord.shift {
                continue;
            }
            let plain_at = at;
            for (other_at, other) in Command::ALL.iter().enumerate() {
                let twin = other.chord();
                if twin.shift && twin.key == chord.key && twin.command == chord.command {
                    assert!(
                        other_at < plain_at,
                        "{other:?} must come before {command:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn no_two_commands_share_a_chord() {
        for (at, a) in Command::ALL.iter().enumerate() {
            for b in &Command::ALL[at + 1..] {
                assert_ne!(a.chord(), b.chord(), "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn only_delete_has_a_second_chord() {
        let with: Vec<_> = Command::ALL.iter().filter(|c| c.also().is_some()).collect();
        assert_eq!(with, [&Command::Delete]);
    }
}
