//! The Settings section at the top of the Recipes panel: the graph's region, the data version and
//! game names when there is a data folder to choose from, and how much of each raw resource the
//! graph's region can supply. Closed, it is one row that names the region. Apart from the region,
//! which is the graph's, these are kept in eframe storage, not in a graph.

use eframe::egui::collapsing_header::CollapsingState;
use eframe::egui::{self, Align, Layout, RichText, TextStyle, TextWrapMode, Ui, WidgetText, vec2};

use crate::data::{DEFAULT_LANGUAGE, DataChoice, DataFolder};
use crate::editor::Editor;
use crate::i18n::Strings;
use crate::theme::{self, color};
use crate::widgets;
use aic_editor::names;

pub use aic_editor::settings::{Availability, BadAmount, STORAGE_KEY, SupplyEdits, parse_amount};

/// The icon beside each raw resource.
const ICON: f32 = 18.0;
/// A raw resource's supply field, frame included.
const SUPPLY_WIDTH: f32 = 56.0;

/// What the Settings section keeps between frames. Whether it is open is egui's, as for any
/// collapsing section.
#[derive(Default)]
pub struct SettingsSection {
    /// What the player typed in the supply fields.
    edits: SupplyEdits,
    /// Opens or closes the section on its next frame.
    open: Option<bool>,
}

impl SettingsSection {
    /// Opens or closes the section, from its next frame.
    pub fn set_open(&mut self, open: bool) {
        self.open = Some(open);
    }
}

/// The section's open state in egui's memory.
fn section_id() -> egui::Id {
    egui::Id::new("settings-section")
}

impl Editor {
    /// The section: a header with the graph's region on its right, and, open, the settings.
    pub(crate) fn settings_section(&mut self, ui: &mut Ui, strings: &Strings) {
        let mut state = CollapsingState::load_with_default_open(ui.ctx(), section_id(), false);
        if let Some(open) = self.settings.open.take() {
            state.set_open(open);
        }
        let catalog = self.session.catalog();
        let region = names::region(catalog, self.session.document().graph().region().as_str());
        let mut title_clicked = false;
        let mut header = state.show_header(ui, |ui| {
            // The title starts where a collapsing header's text does, right after the arrow.
            ui.spacing_mut().button_padding.x = 0.0;
            let title = egui::Button::new(strings.get("settings-title")).frame(false);
            title_clicked = ui.add(title).clicked();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
                ui.label(RichText::new(region).color(color::TEXT_MUTED));
            });
        });
        if title_clicked {
            header.toggle();
        }
        header.body(|ui| self.settings_body(ui, strings));
    }

    fn settings_body(&mut self, ui: &mut Ui, strings: &Strings) {
        ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
        let region_label = strings.get("region-label");
        let data = self.session.data_choice().is_some().then(|| {
            (
                strings.get("settings-data-version"),
                strings.get("settings-language"),
            )
        });
        // The labels share one column; the pickers fill the rest of the row.
        let mut label_width = text_width(ui, &region_label);
        if let Some((version, language)) = &data {
            label_width = label_width
                .max(text_width(ui, version))
                .max(text_width(ui, language));
        }
        labelled(ui, label_width, &region_label, |ui| {
            self.region_picker(ui, strings);
        });
        let mut switch = None;
        if let (Some((version_label, language_label)), (_, _, Some((folder, choice)))) =
            (&data, self.session.settings_parts())
        {
            let labels = [version_label.as_str(), language_label.as_str()];
            switch = data_choice(ui, strings, label_width, labels, folder, choice.clone());
        }
        self.raw_resources(ui, strings);
        if let Some(choice) = switch {
            self.switch_data(choice);
        }
    }

    /// The graph's region. It can change at any time: what the new region lacks shows as warnings
    /// on the cards, and undo takes it back.
    fn region_picker(&mut self, ui: &mut Ui, strings: &Strings) {
        let current = self.session.document().graph().region().clone();
        let mut chosen = None;
        let picker = egui::ComboBox::from_id_salt("region")
            .selected_text(names::region(self.session.catalog(), current.as_str()))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for region in self.session.catalog().regions().keys() {
                    let name = names::region(self.session.catalog(), region.as_str());
                    if ui.selectable_label(*region == current, name).clicked() {
                        chosen = Some(region.clone());
                    }
                }
            });
        let label = strings.get("region-label");
        ui.ctx()
            .accesskit_node_builder(picker.response.id, |node| node.set_label(label));
        if let Some(region) = chosen.filter(|r| *r != current) {
            self.session.set_region(region);
        }
    }

    /// How much of each raw resource the graph's region supplies: one row per resource, its name
    /// cut short to keep the fields in a column.
    fn raw_resources(&mut self, ui: &mut Ui, strings: &Strings) {
        ui.label(theme::strong(strings.get("settings-raw-title")))
            .on_hover_text(strings.get("settings-hint"));
        let region = self.session.document().graph().region().clone();
        let (catalog, availability, _) = self.session.settings_parts();
        let raw = catalog.raw_resources(&region);
        if raw.is_empty() {
            // A sentence, so it wraps to the panel instead of being cut short.
            ui.add(egui::Label::new(RichText::new(strings.get("settings-no-raw")).weak()).wrap());
            return;
        }
        let edits = &mut self.settings.edits;
        let unit = strings.get("settings-per-min");
        let unit_width = text_width(ui, &unit);
        let height = ui.spacing().interact_size.y;
        for item in raw {
            ui.horizontal(|ui| {
                widgets::item_icon(ui, catalog, item.as_str(), ICON);
                let name = names::item(catalog, item.as_str());
                let gap = ui.spacing().item_spacing.x;
                let name_width = ui.available_width() - SUPPLY_WIDTH - unit_width - 2.0 * gap;
                fill_to(ui, name_width.max(0.0), |ui| {
                    ui.add(egui::Label::new(name).truncate());
                });
                let text = edits.field(availability, &region, item);
                let valid = parse_amount(text).is_ok();
                let mut edit = widgets::text_field(ui, text)
                    .font(TextStyle::Monospace)
                    .horizontal_align(Align::Max);
                if !valid {
                    edit = edit.text_color(color::NEGATIVE);
                }
                let response = ui.add_sized(vec2(SUPPLY_WIDTH, height), edit);
                let label = name.to_owned();
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label(label));
                if response.changed() {
                    // Text that doesn't parse stays red and changes nothing.
                    let _ = edits.typed(availability, &region, item);
                }
                ui.label(RichText::new(unit.as_str()).color(color::TEXT_MUTED));
            });
        }
    }
}

/// The width `text` takes on one line in the body font.
fn text_width(ui: &Ui, text: &str) -> f32 {
    WidgetText::from(text)
        .into_galley(
            ui,
            Some(TextWrapMode::Extend),
            f32::INFINITY,
            TextStyle::Body,
        )
        .size()
        .x
}

/// Adds what `add` draws, at most `width` wide, then space up to `width`: one column of a row.
fn fill_to(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    let height = ui.spacing().interact_size.y;
    let drawn = ui.allocate_ui(vec2(width, height), add).response;
    ui.add_space((width - drawn.rect.width()).max(0.0));
}

/// A row with `label` in the label column and what `add` draws filling the rest.
fn labelled(ui: &mut Ui, label_width: f32, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        fill_to(ui, label_width, |ui| {
            ui.label(label);
        });
        add(ui);
    });
}

/// The data version and language pickers. Returns the new choice when one changes.
fn data_choice(
    ui: &mut Ui,
    strings: &Strings,
    label_width: f32,
    [version_label, language_label]: [&str; 2],
    folder: &DataFolder,
    choice: DataChoice,
) -> Option<DataChoice> {
    let mut changed = None;
    let version = folder.version(&choice).name.clone();
    let language = choice
        .language
        .clone()
        .unwrap_or_else(|| DEFAULT_LANGUAGE.to_owned());
    labelled(ui, label_width, version_label, |ui| {
        egui::ComboBox::from_id_salt("data-version")
            .selected_text(version.as_str())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for (i, option) in folder.versions.iter().enumerate() {
                    let mut text = option.name.clone();
                    if i == 0 {
                        text = strings.format("settings-newest", &[("version", &text)]);
                    }
                    if ui.selectable_label(option.name == version, text).clicked() {
                        changed = Some(DataChoice {
                            version: Some(option.name.clone()),
                            ..choice.clone()
                        });
                    }
                }
            });
    });
    labelled(ui, label_width, language_label, |ui| {
        egui::ComboBox::from_id_salt("data-language")
            .selected_text(language.as_str())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for option in folder.languages() {
                    if ui.selectable_label(option == language, &option).clicked() {
                        changed = Some(DataChoice {
                            language: Some(option),
                            ..choice.clone()
                        });
                    }
                }
            });
    });
    changed.filter(|new| *new != choice)
}
