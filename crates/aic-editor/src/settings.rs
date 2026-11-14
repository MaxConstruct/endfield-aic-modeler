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
/// What a UI keeps between runs, as one value: the data choice, the raw resource availability, the
/// recent files, the document open last and whether the status bar shows performance. The egui app
/// keeps these under separate eframe keys; a UI with a settings file writes this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub data: DataChoice,
    pub availability: Availability,
    pub recent_files: Vec<PathBuf>,
    pub last_document: Option<PathBuf>,
    pub show_performance: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            data: DataChoice::default(),
            availability: Availability::default(),
            recent_files: Vec::new(),
            last_document: None,
            show_performance: true,
        }
    }
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
}
