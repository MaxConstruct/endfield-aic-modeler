//! App-wide settings kept in eframe storage, not in a graph: how much of each raw resource every
//! region can supply.

use std::collections::{BTreeMap, HashMap};

use aic_data::{Catalog, ItemId, RegionId};
use eframe::egui::{self, RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::i18n::Strings;
use crate::names;
use crate::widgets::{self, colors};

/// The storage key for [`Availability`].
pub const STORAGE_KEY: &str = "raw-resource-availability";

/// Raw resource supply per region, per minute. A missing entry is unlimited.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Availability(BTreeMap<RegionId, BTreeMap<ItemId, f64>>);

impl Availability {
    pub fn get(&self, region: &RegionId, item: &ItemId) -> Option<f64> {
        self.0.get(region)?.get(item).copied()
    }

    /// Sets an item's supply; `None` makes it unlimited.
    pub fn set(&mut self, region: &RegionId, item: &ItemId, supply: Option<f64>) {
        match supply {
            Some(supply) => {
                self.0
                    .entry(region.clone())
                    .or_default()
                    .insert(item.clone(), supply);
            }
            None => {
                if let Some(items) = self.0.get_mut(region) {
                    items.remove(item);
                }
            }
        }
    }

    /// The supply table of one region, as `check_resources` takes it.
    pub fn region(&self, region: &RegionId) -> HashMap<ItemId, f64> {
        self.0
            .get(region)
            .map(|items| items.iter().map(|(k, v)| (k.clone(), *v)).collect())
            .unwrap_or_default()
    }
}

/// Typed text that isn't a finite number of at least 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadAmount;

/// Parses a typed supply or limit: blank is `None`, otherwise a finite number of at least 0.
pub fn parse_amount(text: &str) -> Result<Option<f64>, BadAmount> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() && value >= 0.0 => Ok(Some(value)),
        _ => Err(BadAmount),
    }
}

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

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        catalog: &Catalog,
        strings: &Strings,
        availability: &mut Availability,
    ) {
        let mut open = self.open;
        egui::Window::new(strings.get("settings-title"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| self.contents(ui, catalog, strings, availability));
        self.open = open;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_is_unlimited_and_bad_input_is_refused() {
        assert_eq!(parse_amount(" "), Ok(None));
        assert_eq!(parse_amount("420"), Ok(Some(420.0)));
        assert_eq!(parse_amount("-1"), Err(BadAmount));
        assert_eq!(parse_amount("lots"), Err(BadAmount));
    }

    #[test]
    fn availability_is_per_region_and_clears() {
        let (wuling, valley) = (RegionId::from("jinlong"), RegionId::from("tundra"));
        let ore = ItemId::from("copper_ore");
        let mut availability = Availability::default();
        availability.set(&wuling, &ore, Some(420.0));
        assert_eq!(availability.get(&wuling, &ore), Some(420.0));
        assert_eq!(availability.get(&valley, &ore), None);
        availability.set(&wuling, &ore, None);
        assert!(availability.region(&wuling).is_empty());
    }
}
