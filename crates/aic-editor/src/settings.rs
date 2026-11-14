//! App-wide settings that aren't in a graph: how much of each raw resource every region can
//! supply, and how a typed amount is read.

use std::collections::{BTreeMap, HashMap};

use aic_data::{ItemId, RegionId};
use serde::{Deserialize, Serialize};

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
