//! App-wide settings kept in eframe storage, not in a graph: how much of each raw resource every
//! region can supply.

use std::collections::HashMap;

use aic_data::{Catalog, ItemId, RegionId};
use eframe::egui::{self, RichText, Ui};

use crate::data::{DEFAULT_LANGUAGE, DataChoice, DataFolder};
use crate::i18n::Strings;
use crate::names;
use crate::widgets::{self, colors};

pub use aic_editor::settings::{Availability, BadAmount, STORAGE_KEY, parse_amount};

/// The settings window and its text fields.
#[derive(Default)]
pub struct SettingsWindow {
    pub open: bool,
    region: Option<RegionId>,
    /// What the player typed, kept while it doesn't parse.
    edits: HashMap<(RegionId, ItemId), String>,
}

impl SettingsWindow {
    /// Opens the window on `region`'s tab.
    pub fn open_on(&mut self, region: &RegionId) {
        self.open = true;
        self.region = Some(region.clone());
    }

    /// Shows the window. Returns the data version and language the player switched to, if they
    /// did; `data` is the data folder and the current choice, when there is a folder to choose from.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        catalog: &Catalog,
        strings: &Strings,
        availability: &mut Availability,
        data: Option<(&DataFolder, DataChoice)>,
    ) -> Option<DataChoice> {
        let mut open = self.open;
        let mut switch = None;
        egui::Window::new(strings.get("settings-title"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                if let Some((folder, choice)) = data {
                    switch = data_choice(ui, strings, folder, choice);
                    ui.separator();
                }
                ui.heading(strings.get("settings-raw-title"));
                self.contents(ui, catalog, strings, availability);
            });
        self.open = open;
        switch
    }

    fn contents(
        &mut self,
        ui: &mut Ui,
        catalog: &Catalog,
        strings: &Strings,
        availability: &mut Availability,
    ) {
        ui.label(RichText::new(strings.get("settings-hint")).weak());
        ui.horizontal(|ui| {
            for region in catalog.regions().keys() {
                let selected = self.region.as_ref() == Some(region);
                if ui
                    .selectable_label(selected, names::region(catalog, region.as_str()))
                    .clicked()
                {
                    self.region = Some(region.clone());
                }
            }
        });
        ui.separator();
        let Some(region) = self
            .region
            .clone()
            .or_else(|| catalog.regions().keys().next().cloned())
        else {
            return;
        };
        let raw = catalog.raw_resources(&region);
        if raw.is_empty() {
            ui.weak(strings.get("settings-no-raw"));
            return;
        }
        egui::Grid::new(("raw-resources", region.as_str()))
            .num_columns(4)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                for item in raw {
                    widgets::item_icon(ui, catalog, item.as_str(), 18.0);
                    let name = ui.label(names::item(catalog, item.as_str()));
                    let key = (region.clone(), item.clone());
                    let text = self.edits.entry(key).or_insert_with(|| {
                        availability
                            .get(&region, item)
                            .map(|v| v.to_string())
                            .unwrap_or_default()
                    });
                    let parsed = parse_amount(text);
                    let mut edit = egui::TextEdit::singleline(text).desired_width(80.0);
                    if parsed.is_err() {
                        edit = edit.text_color(colors::NEGATIVE);
                    }
                    let response = ui.add(edit).labelled_by(name.id);
                    if response.changed()
                        && let Ok(supply) = parse_amount(text)
                    {
                        availability.set(&region, item, supply);
                    }
                    ui.label(strings.get("settings-per-min"));
                    ui.end_row();
                }
            });
    }
}

/// The data version and language pickers. Returns the new choice when one changes.
fn data_choice(
    ui: &mut Ui,
    strings: &Strings,
    folder: &DataFolder,
    choice: DataChoice,
) -> Option<DataChoice> {
    let mut changed = None;
    let version = folder.version(&choice).name.clone();
    let language = choice
        .language
        .clone()
        .unwrap_or_else(|| DEFAULT_LANGUAGE.to_owned());
    egui::Grid::new("data-choice")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(strings.get("settings-data-version"));
            egui::ComboBox::from_id_salt("data-version")
                .selected_text(version.as_str())
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
            ui.end_row();
            ui.label(strings.get("settings-language"));
            egui::ComboBox::from_id_salt("data-language")
                .selected_text(language.as_str())
                .show_ui(ui, |ui| {
                    for option in folder.languages(&choice) {
                        if ui.selectable_label(option == language, &option).clicked() {
                            changed = Some(DataChoice {
                                language: Some(option),
                                ..choice.clone()
                            });
                        }
                    }
                });
            ui.end_row();
        });
    changed.filter(|new| *new != choice)
}
