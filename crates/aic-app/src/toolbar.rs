//! The toolbar beside the menus: a button for every editing command, and the save indicator in the
//! status bar. A key press lights its button for a moment, so the toolbar teaches the keys.

use std::time::Duration;

use eframe::egui::{self, RichText, Ui, vec2};

use crate::editor::Editor;
use crate::i18n::Strings;
use crate::widgets::colors;
use aic_editor::Command;

/// Every text the save indicator shows; it is as wide as the widest.
const SAVE_STATES: [&str; 6] = [
    "save-state-failed",
    "save-state-saving",
    "save-state-unsaved",
    "save-state-draft",
    "save-state-saved",
    "save-state-empty",
];

/// How long a button stays lit after its key is pressed, in seconds.
const FLASH: f64 = 0.4;
const BUTTON: egui::Vec2 = vec2(28.0, 22.0);

/// The toolbar's buttons in order; `None` is a separator. File commands live in the File menu.
const LAYOUT: [Option<Command>; 12] = [
    Some(Command::Undo),
    Some(Command::Redo),
    None,
    Some(Command::Cut),
    Some(Command::Copy),
    Some(Command::Paste),
    Some(Command::Duplicate),
    Some(Command::Delete),
    Some(Command::SelectAll),
    None,
    Some(Command::MakeWorkspace),
    Some(Command::Dissolve),
];

/// The button's picture: a symbol egui's built-in fonts draw (checked by eye in a snapshot;
/// many symbols, such as 📋 and ⧉, are missing from them).
fn icon(command: Command) -> &'static str {
    match command {
        Command::New => "🗋",
        Command::Open => "🗁",
        Command::Save => "💾",
        Command::SaveAs => "🖹",
        Command::Undo => "⟲",
        Command::Redo => "⟳",
        Command::Cut => "✂",
        Command::Copy => "🗐",
        Command::Paste => "🗊",
        Command::Duplicate => "✚",
        Command::Delete => "🗑",
        Command::SelectAll => "⛶",
        Command::MakeWorkspace => "⊞",
        Command::Dissolve => "⊟",
        // Not on the toolbar: in the View menu and the tab menus only.
        Command::SplitRight
        | Command::SplitDown
        | Command::DuplicateView
        | Command::MoveToWindow
        | Command::CloseView => "",
    }
}

impl Editor {
    /// Whether a command can do anything right now; the toolbar and the Edit menu grey it out if not.
    pub(crate) fn can_run(&self, shortcut: Command) -> bool {
        self.session.can_run(shortcut)
    }

    /// The shortcut whose button is lit, if its moment isn't over by egui's clock.
    pub fn flashing(&self, ctx: &egui::Context) -> Option<Command> {
        self.flash_left(ctx).map(|(shortcut, _)| shortcut)
    }

    /// The lit shortcut and how long it has left.
    fn flash_left(&self, ctx: &egui::Context) -> Option<(Command, f64)> {
        let (shortcut, at) = self.flash?;
        let left = FLASH - (ctx.input(|i| i.time) - at);
        (left > 0.0).then_some((shortcut, left))
    }

    pub(crate) fn toolbar(&mut self, ui: &mut Ui, strings: &Strings) {
        let flash = self.flash_left(ui.ctx());
        let lit = flash.map(|(shortcut, _)| shortcut);
        // One more frame when it goes dark; none after that.
        if let Some((_, left)) = flash {
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f64(left));
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for entry in LAYOUT {
                let Some(shortcut) = entry else {
                    ui.separator();
                    continue;
                };
                let keys = ui.ctx().format_shortcut(&crate::edit::keys(shortcut));
                let name = strings.get(shortcut.label_key());
                let button = egui::Button::selectable(
                    lit == Some(shortcut),
                    RichText::new(icon(shortcut)).size(16.0),
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
                    self.run(shortcut);
                }
            }
        });
    }

    /// Whether the work is safe on disk, at the right of the status bar.
    pub(crate) fn save_indicator(&self, ui: &mut Ui, strings: &Strings) {
        let (text, color) = if self.session.save_failed() {
            (strings.get("save-state-failed"), colors::NEGATIVE)
        } else if self.session.document().is_modified() {
            let key = if self.session.document().path().is_some() || self.session.keeps_drafts() {
                "save-state-saving"
            } else {
                "save-state-unsaved"
            };
            (strings.get(key), colors::WARN)
        } else if self.session.document().path().is_some() {
            let key = if self.session.document().is_draft() {
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
            .fold(0.0, f32::max)
            // A label's own width is rounded up to whole points.
            .ceil();
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
            .session
            .document()
            .path()
            .map(|path| self.session.file_label(path))
            .unwrap_or_else(|| strings.get("file-untitled"));
        let when = self
            .session
            .last_saved()
            .map(|at| ago(strings, at.elapsed()));
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
