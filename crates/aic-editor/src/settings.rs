//! App-wide settings that aren't in a graph: how much of each raw resource every region can
//! supply, and how a typed amount is read.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use aic_data::{ItemId, RegionId};
use serde::{Deserialize, Serialize};

use crate::data::DataChoice;

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

/// What the player typed in the raw resource fields, per region and item. Text that doesn't parse
/// stays as typed, so the field keeps showing it.
#[derive(Debug, Clone, Default)]
pub struct SupplyEdits(HashMap<(RegionId, ItemId), String>);

impl SupplyEdits {
    /// The text of `region`'s field for `item`: what was typed there, or else its supply as it is.
    pub fn field(
        &mut self,
        availability: &Availability,
        region: &RegionId,
        item: &ItemId,
    ) -> &mut String {
        self.0
            .entry((region.clone(), item.clone()))
            .or_insert_with(|| {
                availability
                    .get(region, item)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            })
    }

    /// The field's text changed: sets the item's supply from it if it parses (blank is unlimited).
    /// Text that doesn't parse changes nothing.
    pub fn typed(
        &mut self,
        availability: &mut Availability,
        region: &RegionId,
        item: &ItemId,
    ) -> Result<(), BadAmount> {
        let supply = parse_amount(self.field(availability, region, item))?;
        availability.set(region, item, supply);
        Ok(())
    }
}
/// What a UI keeps between runs, as one value: the data choice, the raw resource availability, the
/// recent files, the document open last and whether the status bar shows performance (off until
/// the player turns it on). The egui app keeps these under separate eframe keys; a UI with a
/// settings file writes this.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub data: DataChoice,
    pub availability: Availability,
    pub recent_files: Vec<PathBuf>,
    pub last_document: Option<PathBuf>,
    pub show_performance: bool,
}

impl Settings {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialize")
    }

    /// Settings from text. Anything unreadable is the caller's to replace with the defaults.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
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

    #[test]
    fn a_supply_field_shows_the_supply_and_keeps_what_was_typed_per_region() {
        let (wuling, valley) = (RegionId::from("jinlong"), RegionId::from("tundra"));
        let ore = ItemId::from("copper_ore");
        let mut availability = Availability::default();
        availability.set(&valley, &ore, Some(120.0));
        let mut edits = SupplyEdits::default();
        assert_eq!(edits.field(&availability, &valley, &ore), "120");
        assert_eq!(edits.field(&availability, &wuling, &ore), "");

        "lots".clone_into(edits.field(&availability, &wuling, &ore));
        assert_eq!(
            edits.typed(&mut availability, &wuling, &ore),
            Err(BadAmount)
        );
        assert_eq!(
            availability.get(&wuling, &ore),
            None,
            "bad input changes nothing"
        );
        assert_eq!(edits.field(&availability, &wuling, &ore), "lots");

        "45".clone_into(edits.field(&availability, &wuling, &ore));
        assert_eq!(edits.typed(&mut availability, &wuling, &ore), Ok(()));
        assert_eq!(availability.get(&wuling, &ore), Some(45.0));
        assert_eq!(availability.get(&valley, &ore), Some(120.0));

        "".clone_into(edits.field(&availability, &valley, &ore));
        assert_eq!(edits.typed(&mut availability, &valley, &ore), Ok(()));
        assert_eq!(availability.get(&valley, &ore), None, "blank is unlimited");
    }

    #[test]
    fn settings_round_trip_and_missing_fields_take_defaults() {
        let mut settings = Settings::default();
        settings.recent_files.push("plan.aicm".into());
        settings.show_performance = false;
        settings.availability.set(
            &RegionId::from("jinlong"),
            &ItemId::from("copper_ore"),
            Some(420.0),
        );
        assert_eq!(Settings::from_json(&settings.to_json()).unwrap(), settings);
        assert_eq!(Settings::from_json("{}").unwrap(), Settings::default());
        assert!(Settings::from_json("not json").is_err());
    }

    #[test]
    fn the_performance_readout_is_off_until_turned_on() {
        assert!(!Settings::default().show_performance);
        assert!(!Settings::from_json("{}").unwrap().show_performance);
        let on = Settings::from_json(r#"{ "show_performance": true }"#).unwrap();
        assert!(on.show_performance);
    }
}
