//! The game's exported tables that the extractor reads, as serde structs with only the fields it
//! uses. Every table is a JSON object keyed by ID; `IndexMap` keeps the file's order.
//!
//! A text field is `{"id": <int>, "text": ""}`; its string is `I18nTextTable_<LANG>.json[id]`.

use std::collections::HashMap;
use std::path::Path;

use indexmap::IndexMap;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::ExtractError;

/// A table: rows keyed by their ID.
pub type Table<T> = IndexMap<String, T>;

/// A reference into the text table.
#[derive(Debug, Clone, Deserialize)]
pub struct Text {
    pub id: serde_json::Number,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Craft {
    pub machine_id: String,
    pub formula_group_id: String,
    pub ingredients: Vec<Stacks>,
    pub outcomes: Vec<Stacks>,
    pub progress_round: f64,
    /// 0 for none, else the `genEnv` of a Gas Dispersing Unit group.
    pub gas_env: u32,
    pub formula_desc: Text,
    pub sort_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stacks {
    pub group: Vec<Stack>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stack {
    pub id: String,
    pub count: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftGroup {
    pub ms_per_round: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryItem {
    /// 1 solid, 2 liquid, 4 gas.
    pub phase_type: u32,
    #[serde(default)]
    pub show_in_hub_domain_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRow {
    pub name: Text,
    pub showing_type: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Building {
    pub name: Text,
    pub need_power: bool,
    pub power_consume: f64,
    #[serde(default)]
    pub place_domains: Vec<String>,
    #[serde(default)]
    pub recommend_domains: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildingItem {
    pub building_id: String,
}

/// A transmuting unit's activation input, and also a Gas Dispersing Unit group's gas.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Consumption {
    pub consume_item: String,
    pub consume_rate: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Vaporizer {
    pub groups: Vec<VaporizerGroup>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaporizerGroup {
    #[serde(flatten)]
    pub consumption: Consumption,
    pub gen_env: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Miner {
    pub mineable: Vec<Mineable>,
    pub ms_per_round: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mineable {
    /// `id` is empty when the rig consumes nothing.
    pub consume_item: Stack,
    pub mining_item_id: String,
    pub produce_rate: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pump {
    pub enable_liquid_ids: Vec<String>,
    pub ms_per_round: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerStation {
    pub ms_per_round: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fuel {
    pub power_provide: f64,
    pub progress_round: f64,
}

/// The Water Treatment Unit and the sewage inlet: what they take, one per round.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiquidSink {
    pub liquidable: Vec<String>,
    pub ms_per_round: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SewageExport {
    pub count_cost: f64,
    pub count_produce: f64,
    pub product_item_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SewagePlant {
    pub level_list: Vec<SewagePlantLevel>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SewagePlantLevel {
    /// The level adds an inlet (a sewage inlet gate) rather than the outlet.
    pub is_import_level: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settlement {
    pub domain_id: String,
    /// Keyed by the level, `"1"`, `"2"`, …
    pub settlement_level_map: IndexMap<String, SettlementLevel>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementLevel {
    #[serde(default)]
    pub settlement_trade_item_map: IndexMap<String, TradeItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeItem {
    pub reward_money_count: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Domain {
    pub domain_name: Text,
    pub domain_gold_item_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowingType {
    pub name: Text,
    pub sort_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Belt {
    pub belt_data: Rounds,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pipe {
    pub pipe_data: Rounds,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rounds {
    pub ms_per_round: f64,
}

/// A Forge of Luminance (`FactoryFormationStakeTable`): the gas it takes, and the arrays it
/// projects, one per area, each of a kind the player picks.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forge {
    /// Per minute, as the Gas Dispersing Unit's (`consumeRateUpperLimit` only caps the intake).
    #[serde(flatten)]
    pub consumption: Consumption,
    /// One area per array it projects.
    #[serde(rename = "ranges")]
    pub areas: Vec<Area>,
    /// The kinds of array (`FactoryFormationTable` IDs) each area can be.
    #[serde(rename = "selectableFormationIds")]
    pub kinds: Vec<String>,
}

/// An array's area, in tiles.
#[derive(Debug, Clone, Deserialize)]
pub struct Area {
    pub width: f64,
    pub depth: f64,
}

/// A kind of array (`FactoryFormationTable`). Only its ID is used.
#[derive(Debug, Clone, Deserialize)]
pub struct ArrayKind {}

/// A kind of array's tiers (`FactoryFormationLevelTable`), keyed by tier, `"1"`, `"2"`, …
#[derive(Debug, Clone, Deserialize)]
pub struct ArrayTiers {
    #[serde(rename = "levels")]
    pub tiers: IndexMap<String, ArrayTier>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArrayTier {
    #[serde(rename = "level")]
    pub tier: u32,
    /// "L-Array T1".
    pub name: Text,
}

/// What an array of one kind turns an item into (`FactoryFormationTransferTable`), with the
/// chance per pulse at each tier.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrayConversion {
    #[serde(rename = "formationId")]
    pub kind: String,
    pub from_item_id: String,
    pub to_item_id: String,
    /// In ten-thousandths, lowest tier first.
    #[serde(rename = "transferChances")]
    pub chances: Vec<f64>,
}

/// Arrays of several kinds stacked to make one of another (`FactoryFormationCombinationTable`):
/// an L and a T make an E.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrayCombination {
    #[serde(rename = "sourceFormationIds")]
    pub sources: Vec<String>,
    #[serde(rename = "targetFormationId")]
    pub target: String,
}

/// `FactoryConst`: one object of the factory's constants. Only those the extractor uses.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryConst {
    /// Milliseconds between two pulses of an array.
    #[serde(rename = "facFormationEffectInterval")]
    pub array_pulse_ms: Option<f64>,
}

/// Every table the extractor reads, from one folder of exported tables.
#[derive(Debug, Clone)]
pub struct Tables {
    pub crafts: Table<Craft>,
    pub craft_groups: Table<CraftGroup>,
    pub factory_items: Table<FactoryItem>,
    pub items: Table<ItemRow>,
    pub buildings: Table<Building>,
    pub building_items: Table<BuildingItem>,
    pub transmuters: Table<Consumption>,
    pub vaporizers: Table<Vaporizer>,
    pub miners: Table<Miner>,
    pub gas_miners: Table<Miner>,
    pub pumps: Table<Pump>,
    pub power_stations: Table<PowerStation>,
    pub fuels: Table<Fuel>,
    pub fluid_consumers: Table<LiquidSink>,
    pub sewage_imports: Table<LiquidSink>,
    pub sewage_exports: Table<SewageExport>,
    pub sewage_plants: Table<SewagePlant>,
    pub settlements: Table<Settlement>,
    pub domains: Table<Domain>,
    pub showing_types: Table<ShowingType>,
    pub belts: Table<Belt>,
    pub pipes: Table<Pipe>,
    /// The array tables, which versions before v1.7.0 lack: empty where a version has none.
    /// `FactoryFormationStakeTable`.
    pub forges: Table<Forge>,
    /// `FactoryFormationTable`.
    pub array_kinds: Table<ArrayKind>,
    /// `FactoryFormationLevelTable`, by kind.
    pub array_tiers: Table<ArrayTiers>,
    /// `FactoryFormationTransferTable`.
    pub array_conversions: Table<ArrayConversion>,
    /// `FactoryFormationCombinationTable`.
    pub array_combinations: Table<ArrayCombination>,
    /// `FactoryConst`; all defaults where a version has none.
    pub constants: FactoryConst,
    /// `I18nTextTable_<LANG>`: text ID to string.
    pub text: HashMap<String, String>,
    /// The text table's file name, for errors.
    pub text_table: String,
}

impl Tables {
    /// Reads every table from `dir`, with the text of `lang` (`EN`).
    pub fn read(dir: &Path, lang: &str) -> Result<Self, ExtractError> {
        let text_table = format!("I18nTextTable_{lang}");
        Ok(Self {
            crafts: read(dir, "FactoryMachineCraftTable")?,
            craft_groups: read(dir, "FactoryMachineCraftGroupTable")?,
            factory_items: read(dir, "FactoryItemTable")?,
            items: read(dir, "ItemTable")?,
            buildings: read(dir, "FactoryBuildingTable")?,
            building_items: read(dir, "FactoryBuildingItemTable")?,
            transmuters: read(dir, "FactoryTransmuterTable")?,
            vaporizers: read(dir, "FactoryVaporizerTable")?,
            miners: read(dir, "FactoryMinerTable")?,
            gas_miners: read(dir, "FactoryGasMinerTable")?,
            pumps: read(dir, "FactoryFluidPumpInTable")?,
            power_stations: read(dir, "FactoryPowerStationTable")?,
            fuels: read(dir, "FactoryFuelItemTable")?,
            fluid_consumers: read(dir, "FactoryFluidConsumeTable")?,
            sewage_imports: read(dir, "FactorySewageTreatImportTable")?,
            sewage_exports: read(dir, "FactorySewageTreatExportTable")?,
            sewage_plants: read(dir, "FactorySewageTreatPlantStoreTable")?,
            settlements: read(dir, "SettlementBasicDataTable")?,
            domains: read(dir, "DomainDataTable")?,
            showing_types: read(dir, "ItemShowingTypeTable")?,
            belts: read(dir, "FactoryGridBeltTable")?,
            pipes: read(dir, "FactoryLiquidPipeTable")?,
            forges: read_optional(dir, "FactoryFormationStakeTable")?,
            array_kinds: read_optional(dir, "FactoryFormationTable")?,
            array_tiers: read_optional(dir, "FactoryFormationLevelTable")?,
            array_conversions: read_optional(dir, "FactoryFormationTransferTable")?,
            array_combinations: read_optional(dir, "FactoryFormationCombinationTable")?,
            constants: read_optional(dir, "FactoryConst")?,
            text: read(dir, &text_table)?,
            text_table: format!("{text_table}.json"),
        })
    }

    /// The string a text field names. Text the table lacks, or leaves empty, is `None`.
    pub fn text(&self, text: &Text) -> Option<&str> {
        let key = text.id.to_string();
        self.text
            .get(&key)
            .map(String::as_str)
            .filter(|s| !s.trim().is_empty())
    }
}

/// `<dir>/<name>.json`.
fn read<T: DeserializeOwned>(dir: &Path, name: &str) -> Result<T, ExtractError> {
    let file = dir.join(format!("{name}.json"));
    let bytes = std::fs::read(&file).map_err(|source| ExtractError::Read {
        file: file.clone(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| ExtractError::Json { file, source })
}

/// `<dir>/<name>.json`, or the default (an empty table) when the folder has no such file: a table
/// an older version of the game doesn't have.
fn read_optional<T: DeserializeOwned + Default>(dir: &Path, name: &str) -> Result<T, ExtractError> {
    if dir.join(format!("{name}.json")).is_file() {
        read(dir, name)
    } else {
        Ok(T::default())
    }
}
