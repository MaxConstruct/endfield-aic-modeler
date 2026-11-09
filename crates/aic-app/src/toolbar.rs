//! The toolbar beside the menus: a button for every editing command, and the save indicator in the
//! status bar. A key press lights its button for a moment, so the toolbar teaches the keys.

use std::path::Path;
use std::time::Duration;

use eframe::egui::{self, RichText, Ui, vec2};

use crate::canvas::Selection;
use crate::edit::Shortcut;
use crate::editor::Editor;
use crate::i18n::Strings;
use crate::widgets::colors;

/// Every text the save indicator shows; it is as wide as the widest.
const SAVE_STATES: [&str; 6] = [
    "save-state-failed",
    "save-state-saving",
    "save-state-unsaved",
    "save-state-draft",
    "save-state-saved",
    "save-state-empty",
];

/// How long a button stays lit after its key is pressed.
const FLASH: Duration = Duration::from_millis(400);
const BUTTON: egui::Vec2 = vec2(28.0, 22.0);

/// The toolbar's buttons in order; `None` is a separator. File commands live in the File menu.
const LAYOUT: [Option<Shortcut>; 12] = [
    Some(Shortcut::Undo),
    Some(Shortcut::Redo),
    None,
    Some(Shortcut::Cut),
    Some(Shortcut::Copy),
    Some(Shortcut::Paste),
    Some(Shortcut::Duplicate),
    Some(Shortcut::Delete),
    Some(Shortcut::SelectAll),
    None,
    Some(Shortcut::MakeWorkspace),
    Some(Shortcut::Dissolve),
];

impl Shortcut {
    /// The button's picture: a symbol egui's built-in fonts draw (checked by eye in a snapshot;
    /// many symbols, such as 📋 and ⧉, are missing from them).
    fn icon(self) -> &'static str {
        match self {
            Self::New => "🗋",
            Self::Open => "🗁",
            Self::Save => "💾",
            Self::SaveAs => "🖹",
            Self::Undo => "⟲",
            Self::Redo => "⟳",
            Self::Cut => "✂",
            Self::Copy => "🗐",
            Self::Paste => "🗊",
            Self::Duplicate => "✚",
            Self::Delete => "🗑",
            Self::SelectAll => "⛶",
            Self::MakeWorkspace => "⊞",
            Self::Dissolve => "⊟",
        }
    }

    /// The Fluent key of its name, shared with the menus.
    pub(crate) fn name_key(self) -> &'static str {
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
        }
    }
}

impl Editor {
    /// Whether a command can do anything right now; the toolbar and the Edit menu grey it out if not.
    pub(crate) fn can_run(&self, shortcut: Shortcut) -> bool {
        match shortcut {
            Shortcut::Undo => self.doc.can_undo(),
            Shortcut::Redo => self.doc.can_redo(),
            Shortcut::Cut | Shortcut::Copy | Shortcut::Duplicate | Shortcut::MakeWorkspace => {
                self.has_selected_nodes()
            }
            Shortcut::Paste => self.clipboard.is_some(),
            Shortcut::Delete => self.selection.is_some(),
            Shortcut::Dissolve => self
                .selection
                .as_ref()
                .and_then(Selection::single_workspace)
                .is_some(),
            Shortcut::New
            | Shortcut::Open
            | Shortcut::Save
            | Shortcut::SaveAs
            | Shortcut::SelectAll => true,
        }
    }

    /// The shortcut whose button is lit, if its moment isn't over.
    pub fn flashing(&self) -> Option<Shortcut> {
        self.flash
            .filter(|(_, at)| at.elapsed() < FLASH)
            .map(|(shortcut, _)| shortcut)
    }

    pub(crate) fn toolbar(&mut self, ui: &mut Ui, strings: &Strings) {
        let lit = self.flashing();
        if lit.is_some() {
            ui.ctx().request_repaint_after(FLASH);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for entry in LAYOUT {
                let Some(shortcut) = entry else {
                    ui.separator();
                    continue;
                };
                let keys = ui.ctx().format_shortcut(&shortcut.keys());
                let name = strings.get(shortcut.name_key());
                let button = egui::Button::selectable(
                    lit == Some(shortcut),
                    RichText::new(shortcut.icon()).size(16.0),
                )
                .min_size(BUTTON);
                let response = ui
                    .add_enabled(self.can_run(shortcut), button)
                    .on_hover_text(
                        strings.format("toolbar-hint", &[("name", &name), ("keys", &keys)]),
                    )
                    .on_disabled_hover_text(
                        strings.format("toolbar-hint", &[("name", &name), ("keys", &keys)]),
                    );
                let label = name.clone();
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label(label));
                if response.clicked() {
                    self.run(shortcut, strings);
                }
            }
        });
    }

    /// Whether the work is safe on disk, at the right of the status bar.
    pub(crate) fn save_indicator(&self, ui: &mut Ui, strings: &Strings) {
        let (text, color) = if self.save_failed {
            (strings.get("save-state-failed"), colors::NEGATIVE)
        } else if self.doc.is_modified() {
            let key = if self.doc.path().is_some() || self.drafts_dir.is_some() {
                "save-state-saving"
            } else {
                "save-state-unsaved"
            };
            (strings.get(key), colors::WARN)
        } else if self.doc.path().is_some() {
            let key = if self.doc.is_draft() {
                "save-state-draft"
            } else {
                "save-state-saved"
            };
            (strings.get(key), colors::POSITIVE)
        } else {
            (
                strings.get("save-state-empty"),
                ui.visuals().weak_text_color(),
            )
        };
        // A fixed width, so what sits beside it doesn't move as the state changes.
        let widest = SAVE_STATES
            .iter()
            .map(|key| {
                egui::WidgetText::from(strings.get(key))
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Body,
                    )
                    .size()
                    .x
            })
            .fold(0.0, f32::max);
        let response = ui
            .allocate_ui_with_layout(
                vec2(widest, ui.available_height()),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.set_min_width(widest);
                    ui.label(RichText::new(text).color(color))
                },
            )
            .inner;
        let file = self
            .doc
            .path()
            .map(Path::display)
            .map(|p| p.to_string())
            .unwrap_or_else(|| strings.get("file-untitled"));
        let when = self.last_saved.map(|at| ago(strings, at.elapsed()));
        let hint = match when {
            Some(when) => strings.format("save-state-hint", &[("file", &file), ("when", &when)]),
            None => file,
        };
        response.on_hover_text(hint);
    }
}

/// "just now", "2 min ago".
fn ago(strings: &Strings, elapsed: Duration) -> String {
    let minutes = elapsed.as_secs() / 60;
    if minutes == 0 {
        strings.get("save-state-just-now")
    } else {
        strings.format("save-state-minutes", &[("minutes", &minutes.to_string())])
    }
}
