//! Tables in, data file out: turns the game's tables into a data version and the names it needs.

use std::collections::{BTreeSet, HashMap, HashSet};

use aic_data::Phase;
use aic_data::file::{
    ActivationEntry, Aliases, DataFile, FORMAT, FacilityEntry, ItemEntry, RecipeEntry, RecipeKind,
};
use indexmap::IndexMap;

use crate::error::ExtractError;
use crate::names::{NameKind, NamesFile};
use crate::report::{
    AliasKind, ArraysReport, DroppedAlias, PriceConflict, Report, SharedName, Unaliased,
};
use crate::tables::{Stacks, Tables};

/// The game's domains and the region IDs the app has always used for them.
pub const REGIONS: [(&str, &str); 2] = [("domain_1", "tundra"), ("domain_2", "jinlong")];

/// Buildings that no placeable item stands for, and the facility IDs the app already gives them.
/// Any other building without an item keeps its building ID.
pub const ITEMLESS_FACILITIES: [(&str, &str); 2] = [
    ("liquid_clean_gate_1", "liquid_clean_gate"),
    ("liquid_recycle_gate_1", "liquid_recycle_gate"),
];

/// Liquids found in the world, which a pump lifts as a raw resource. A pump's
/// `enableLiquidIds` also lists every liquid it can draw from a player's tank (sewage, liquid
/// plants, liquid xiranite, …); those are made, not mined, so they get no mining recipe.
pub const NATURAL_LIQUIDS: [&str; 2] = ["item_liquid_water", "item_liquid_acid"];

/// Item IDs that the factoriolab data invented, mapped to the game's IDs for the same items.
/// Emitted as aliases when the previous version has the old ID and this one the new.
pub const ITEM_ALIASES: [(&str, &str); 7] = [
    ("iron_bottle-liquid_plant_grass_1", "fbottle_iron_grass_1"),
    ("iron_bottle-liquid_plant_grass_2", "fbottle_iron_grass_2"),
    (
        "copper_bottle-liquid_plant_grass_1",
        "fbottle_copper_grass_1",
    ),
    (
        "copper_bottle-liquid_plant_grass_2",
        "fbottle_copper_grass_2",
    ),
    ("tundra_coupon", "domain_tundra_coupon"),
    ("jinlong_coupon", "domain_jinlong_coupon"),
    ("__miner_water", "liquid_water"),
];

/// Seconds per settlement trade. A trade is not timed in the game: an outpost takes what it is
/// sent. Only its coupons per item matter (the app places it as an outpost order, a sink).
pub const TRADE_TIME_S: f64 = 2.0;

/// The category of the Thermal Bank's power recipes.
pub const GEN_POWER: &str = "gen-power";
/// The category of settlement trades.
pub const SETTLEMENT: &str = "settlement";

/// What the extractor made: the data file, ready to write, and what it found worth saying.
#[derive(Debug, Clone)]
pub struct Extraction {
    pub data: DataFile,
    pub report: Report,
}

/// Turns `tables` into data version `version`. Names the data needs and `names` lacks are added to
/// it. With `previous` (an older data version), items keep its colours and the data gets aliases
/// from its IDs.
pub fn extract(
    tables: &Tables,
    previous: Option<&DataFile>,
    names: &mut NamesFile,
    version: &str,
) -> Result<Extraction, ExtractError> {
    let mut extractor = Extractor {
        tables,
        facilities: IndexMap::new(),
        renamed: HashSet::new(),
        array_crafts: IndexMap::new(),
        report: Report::default(),
    };
    let mut drafts = Vec::new();
    extractor.mining(&mut drafts)?;
    extractor.crafts(&mut drafts)?;
    extractor.power(&mut drafts)?;
    extractor.liquid_sinks(&mut drafts)?;
    extractor.sewage_export(&mut drafts)?;
    extractor.vaporizers(&mut drafts)?;
    extractor.arrays(&mut drafts)?;
    extractor.trades(&mut drafts)?;
    extractor.packaging(&mut drafts);
    extractor.finish(drafts, previous, names, version)
}

/// A recipe while it is being gathered. Items are the game's IDs (`item_x`).
#[derive(Debug, Clone)]
struct Draft {
    id: String,
    /// The game's name, for the names table.
    name: Option<String>,
    category: String,
    time_s: f64,
    inputs: Vec<(String, f64)>,
    outputs: Vec<(String, f64)>,
    /// Facility IDs.
    facilities: Vec<String>,
    power_kw: Option<f64>,
    kind: Option<RecipeKind>,
    /// A gas item, game ID.
    environment: Option<String>,
    /// The gas whose environment it spreads (a Gas Dispersing Unit's), game ID.
    creates_environment: Option<String>,
    /// What one facility running it projects (a forge's): T1 arrays by facility ID, how many.
    projects: Vec<(String, u32)>,
    /// Set for trades; otherwise worked out from the items.
    regions: Option<Vec<String>>,
    /// Which part of the file the recipe goes in: mining, crafts, the other buildings, trades.
    group: Group,
    /// Within its group and facility: the game's `sortId` for crafts, 0 otherwise.
    sort_id: i64,
}

/// In the order the file lists them when there is no previous version to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Mining,
    Craft,
    Other,
    /// A craft that fills a container with a fluid, which another craft empties again.
    Filling,
    /// That other craft.
    Emptying,
    Trade,
}

impl Group {
    /// The game's crafts, which it orders by building and `sortId`.
    fn is_craft(self) -> bool {
        matches!(self, Self::Craft | Self::Filling | Self::Emptying)
    }

    /// Kept out of the way at the end of the file: there is one filling and one emptying for
    /// every container and fluid, nearly as many as every other recipe together.
    fn is_packaging(self) -> bool {
        matches!(self, Self::Filling | Self::Emptying)
    }
}

impl Draft {
    /// Two drafts that make the same thing at the same rate are one recipe on several facilities.
    fn same_work(&self, other: &Draft) -> bool {
        self.group == other.group
            && self.kind == other.kind
            && self.time_s == other.time_s
            && self.inputs == other.inputs
            && self.outputs == other.outputs
            && self.environment == other.environment
            && self.creates_environment == other.creates_environment
            && self.projects == other.projects
            && self.power_kw == other.power_kw
    }
}

/// Adds `draft`, or its facilities to an earlier draft that does the same work: the game has the
/// same craft once per mixing pool, and the same ore on each mining rig. The first keeps its ID
/// and name.
fn push_merging(drafts: &mut Vec<Draft>, draft: Draft) {
    match drafts.iter_mut().find(|d| d.same_work(&draft)) {
        Some(earlier) => {
            for facility in draft.facilities {
                if !earlier.facilities.contains(&facility) {
                    earlier.facilities.push(facility);
                }
            }
        }
        None => drafts.push(draft),
    }
}

struct FacilityDraft {
    entry: FacilityEntry,
    name: Option<String>,
    /// Whether `name` is the game's own name for the building. It then takes the place of a name
    /// the table has; a name of our making (an outpost's) is only added where there is none.
    named_by_game: bool,
    /// Its building's place in `FactoryBuildingTable`; synthetic outposts come after them all.
    order: usize,
}

struct Extractor<'a> {
    tables: &'a Tables,
    /// By facility ID.
    facilities: IndexMap<String, FacilityDraft>,
    /// Recipes whose name is of our making and takes the place of one the names table has.
    renamed: HashSet<String>,
    /// The crafts the game lists for the arrays' conversions, one per tier, left out: by the
    /// facility they were on, how many.
    array_crafts: IndexMap<String, usize>,
    report: Report,
}

/// `item_copper_nugget` → `copper_nugget`.
fn item_id(table: &str, key: &str, game_id: &str) -> Result<String, ExtractError> {
    game_id
        .strip_prefix("item_")
        .map(str::to_owned)
        .ok_or_else(|| ExtractError::table(table, key, format!("`{game_id}` is not an item ID")))
}

fn region_of(table: &str, key: &str, domain: &str) -> Result<&'static str, ExtractError> {
    REGIONS
        .iter()
        .find(|(d, _)| *d == domain)
        .map(|(_, region)| *region)
        .ok_or_else(|| {
            ExtractError::table(
                table,
                key,
                format!("unknown domain `{domain}`; the extractor knows domain_1 and domain_2"),
            )
        })
}

fn stacks(groups: &[Stacks]) -> Vec<(String, f64)> {
    groups
        .iter()
        .flat_map(|stacks| &stacks.group)
        .map(|stack| (stack.id.clone(), stack.count))
        .collect()
}

impl Extractor<'_> {
    fn missing(table: &str, key: &str, what: &str) -> ExtractError {
        ExtractError::table(table, key, format!("not in the table ({what})"))
    }

    /// The facility a building is, registered on first use.
    fn facility(&mut self, building: &str) -> Result<String, ExtractError> {
        let tables = self.tables;
        let id = match ITEMLESS_FACILITIES.iter().find(|(b, _)| *b == building) {
            Some((_, id)) => (*id).to_owned(),
            None => tables
                .building_items
                .iter()
                .find(|(_, row)| row.building_id == building)
                .map(|(item, _)| {
                    item.strip_prefix("item_port_")
                        .unwrap_or(building)
                        .to_owned()
                })
                .unwrap_or_else(|| building.to_owned()),
        };
        if self.facilities.contains_key(&id) {
            return Ok(id);
        }
        let (order, _, row) = tables.buildings.get_full(building).ok_or_else(|| {
            Self::missing("FactoryBuildingTable", building, "a facility of a recipe")
        })?;
        let domains = if row.place_domains.is_empty() {
            &row.recommend_domains
        } else {
            &row.place_domains
        };
        let regions = self.regions_of("FactoryBuildingTable", building, domains)?;
        let activation = match tables.transmuters.get(building) {
            Some(t) => Some(ActivationEntry {
                item: item_id("FactoryTransmuterTable", building, &t.consume_item)?,
                per_min: t.consume_rate,
            }),
            None => None,
        };
        let entry = FacilityEntry {
            id: id.clone(),
            speed: 1.0,
            power_kw: if row.need_power {
                row.power_consume
            } else {
                0.0
            },
            regions,
            activation,
            whole_units: false,
            stack: IndexMap::new(),
        };
        let name = tables.text(&row.name).map(str::to_owned);
        self.facilities.insert(
            id.clone(),
            FacilityDraft {
                entry,
                name,
                named_by_game: true,
                order,
            },
        );
        Ok(id)
    }

    /// Region IDs for a list of domains: empty when it covers every region.
    fn regions_of(
        &self,
        table: &str,
        key: &str,
        domains: &[String],
    ) -> Result<Vec<String>, ExtractError> {
        let mut regions = BTreeSet::new();
        for domain in domains {
            regions.insert(region_of(table, key, domain)?);
        }
        Ok(ordered_regions(&regions))
    }

    fn item_name(&self, game_id: &str) -> Option<String> {
        let row = self.tables.items.get(game_id)?;
        self.tables.text(&row.name).map(str::to_owned)
    }

    /// `type_<n>`, where n is the item's showing type in `ItemTable`.
    fn category_of(&self, game_id: &str, recipe: &str) -> Result<String, ExtractError> {
        let row = self.tables.items.get(game_id).ok_or_else(|| {
            Self::missing(
                "ItemTable",
                game_id,
                &format!("an item of recipe `{recipe}`"),
            )
        })?;
        Ok(format!("type_{}", row.showing_type))
    }

    /// The category of a recipe: its first output's, else its first input's.
    fn category_of_work(
        &self,
        inputs: &[(String, f64)],
        outputs: &[(String, f64)],
        recipe: &str,
    ) -> Result<String, ExtractError> {
        let first = outputs.first().or(inputs.first()).ok_or_else(|| {
            ExtractError::table("FactoryMachineCraftTable", recipe, "has no items")
        })?;
        self.category_of(&first.0, recipe)
    }

    /// Mining rigs, the gas extractor and pumps: each thing they lift, per round.
    fn mining(&mut self, drafts: &mut Vec<Draft>) -> Result<(), ExtractError> {
        let tables = self.tables;
        let rigs = (tables.miners.iter().map(|r| ("FactoryMinerTable", r))).chain(
            tables
                .gas_miners
                .iter()
                .map(|r| ("FactoryGasMinerTable", r)),
        );
        for (table, (building, rig)) in rigs {
            let facility = self.facility(building)?;
            for mineable in &rig.mineable {
                let output = item_id(table, building, &mineable.mining_item_id)?;
                let consumes = !mineable.consume_item.id.is_empty();
                let (id, inputs) = if consumes {
                    let input = &mineable.consume_item;
                    let input_id = item_id(table, building, &input.id)?;
                    (
                        format!("{output}-{input_id}"),
                        vec![(input.id.clone(), input.count)],
                    )
                } else {
                    (output, vec![])
                };
                let outputs = vec![(mineable.mining_item_id.clone(), mineable.produce_rate)];
                let category = self.category_of_work(&inputs, &outputs, &id)?;
                push_merging(
                    drafts,
                    Draft {
                        id,
                        name: self.item_name(&mineable.mining_item_id),
                        category,
                        time_s: rig.ms_per_round / 1000.0,
                        inputs,
                        outputs,
                        facilities: vec![facility.clone()],
                        power_kw: None,
                        kind: Some(RecipeKind::Mining),
                        environment: None,
                        creates_environment: None,
                        projects: Vec::new(),
                        regions: None,
                        group: Group::Mining,
                        sort_id: 0,
                    },
                );
            }
        }
        for (building, pump) in &tables.pumps {
            let liquids = (pump.enable_liquid_ids.iter())
                .filter(|liquid| NATURAL_LIQUIDS.contains(&liquid.as_str()));
            for liquid in liquids {
                let id = item_id("FactoryFluidPumpInTable", building, liquid)?;
                let facility = self.facility(building)?;
                let outputs = vec![(liquid.clone(), 1.0)];
                let category = self.category_of_work(&[], &outputs, &id)?;
                push_merging(
                    drafts,
                    Draft {
                        id,
                        name: self.item_name(liquid),
                        category,
                        time_s: pump.ms_per_round / 1000.0,
                        inputs: vec![],
                        outputs,
                        facilities: vec![facility],
                        power_kw: None,
                        kind: Some(RecipeKind::Mining),
                        environment: None,
                        creates_environment: None,
                        projects: Vec::new(),
                        regions: None,
                        group: Group::Mining,
                        sort_id: 0,
                    },
                );
            }
        }
        Ok(())
    }

    /// The gas each environment number stands for: the Gas Dispersing Unit's groups.
    fn environments(&self) -> HashMap<u32, String> {
        (self.tables.vaporizers.values())
            .flat_map(|v| &v.groups)
            .map(|g| (g.gen_env, g.consumption.consume_item.clone()))
            .collect()
    }

    fn crafts(&mut self, drafts: &mut Vec<Draft>) -> Result<(), ExtractError> {
        const TABLE: &str = "FactoryMachineCraftTable";
        let tables = self.tables;
        let environments = self.environments();
        let array_crafts = array_craft_ids(tables);
        for (id, craft) in &tables.crafts {
            if array_crafts.contains(id.as_str()) {
                // An array's conversion at one tier: `arrays` writes it, once, on the array.
                let facility = self.facility(&craft.machine_id)?;
                *self.array_crafts.entry(facility).or_default() += 1;
                continue;
            }
            let group = tables
                .craft_groups
                .get(&craft.formula_group_id)
                .ok_or_else(|| {
                    Self::missing(
                        "FactoryMachineCraftGroupTable",
                        &craft.formula_group_id,
                        &format!("the formula group of craft `{id}`"),
                    )
                })?;
            let environment = match craft.gas_env {
                0 => None,
                n => Some(environments.get(&n).cloned().ok_or_else(|| {
                    ExtractError::table(
                        TABLE,
                        id,
                        format!("gas environment {n} is no group of FactoryVaporizerTable"),
                    )
                })?),
            };
            let facility = self.facility(&craft.machine_id)?;
            let inputs = stacks(&craft.ingredients);
            let outputs = stacks(&craft.outcomes);
            let category = self.category_of_work(&inputs, &outputs, id)?;
            push_merging(
                drafts,
                Draft {
                    id: id.clone(),
                    name: tables.text(&craft.formula_desc).map(str::to_owned),
                    category,
                    time_s: craft.progress_round * group.ms_per_round / 1000.0,
                    inputs,
                    outputs,
                    facilities: vec![facility],
                    power_kw: None,
                    kind: None,
                    environment,
                    creates_environment: None,
                    projects: Vec::new(),
                    regions: None,
                    group: Group::Craft,
                    sort_id: craft.sort_id,
                },
            );
        }
        Ok(())
    }

    /// The Thermal Bank: one fuel item a round, for its power.
    fn power(&mut self, drafts: &mut Vec<Draft>) -> Result<(), ExtractError> {
        let tables = self.tables;
        for (building, station) in &tables.power_stations {
            let facility = self.facility(building)?;
            for (fuel_id, fuel) in &tables.fuels {
                let item = item_id("FactoryFuelItemTable", fuel_id, fuel_id)?;
                let name = self.item_name(fuel_id).map(|n| format!("Power({n})"));
                push_merging(
                    drafts,
                    Draft {
                        id: format!("power_{item}"),
                        name,
                        category: GEN_POWER.to_owned(),
                        time_s: fuel.progress_round * station.ms_per_round / 1000.0,
                        inputs: vec![(fuel_id.clone(), 1.0)],
                        outputs: vec![],
                        facilities: vec![facility.clone()],
                        power_kw: Some(-fuel.power_provide),
                        kind: None,
                        environment: None,
                        creates_environment: None,
                        projects: Vec::new(),
                        regions: None,
                        group: Group::Other,
                        sort_id: 0,
                    },
                );
            }
        }
        Ok(())
    }

    /// A recipe that takes `amount` of one item per `time_s` and makes nothing:
    /// `<facility>-<item>`, named `<facility>(<item>)`.
    fn sink(
        &mut self,
        table: &str,
        building: &str,
        item: &str,
        amount: f64,
        time_s: f64,
    ) -> Result<Draft, ExtractError> {
        let facility = self.facility(building)?;
        let id = format!("{facility}-{}", item_id(table, building, item)?);
        let inputs = vec![(item.to_owned(), amount)];
        let category = self.category_of_work(&inputs, &[], &id)?;
        let name = match (&self.facilities[&facility].name, self.item_name(item)) {
            (Some(f), Some(i)) => Some(format!("{f}({i})")),
            _ => None,
        };
        Ok(Draft {
            id,
            name,
            category,
            time_s,
            inputs,
            outputs: vec![],
            facilities: vec![facility],
            power_kw: None,
            kind: None,
            environment: None,
            creates_environment: None,
            projects: Vec::new(),
            regions: None,
            group: Group::Other,
            sort_id: 0,
        })
    }

    /// The Water Treatment Unit and the sewage inlet: one liquid in a round, nothing out.
    fn liquid_sinks(&mut self, drafts: &mut Vec<Draft>) -> Result<(), ExtractError> {
        let tables = self.tables;
        let sinks = (tables
            .fluid_consumers
            .iter()
            .map(|r| ("FactoryFluidConsumeTable", r)))
        .chain((tables.sewage_imports.iter()).map(|r| ("FactorySewageTreatImportTable", r)));
        for (table, (building, sink)) in sinks {
            for liquid in &sink.liquidable {
                let draft = self.sink(table, building, liquid, 1.0, sink.ms_per_round / 1000.0)?;
                push_merging(drafts, draft);
            }
        }
        Ok(())
    }

    /// The byproduct outlet of the sewage plant: `countCost` sewage in, `countProduce` of its
    /// product out.
    ///
    /// The table gives no time. The plant's levels (`FactorySewageTreatPlantStoreTable`) add its
    /// inlets, each of which takes in sewage at the inlet's rate, then the outlet: the outlet is
    /// taken to work up what all the inlets bring in, so one craft takes `countCost` over their
    /// summed rate (30 over 3 × 2 a second: 5 s).
    fn sewage_export(&mut self, drafts: &mut Vec<Draft>) -> Result<(), ExtractError> {
        const TABLE: &str = "FactorySewageTreatExportTable";
        let tables = self.tables;
        for (building, export) in &tables.sewage_exports {
            let (import_building, import) = tables.sewage_imports.first().ok_or_else(|| {
                ExtractError::table(TABLE, building, "no sewage inlet to feed it")
            })?;
            let [sewage] = import.liquidable.as_slice() else {
                return Err(ExtractError::table(
                    "FactorySewageTreatImportTable",
                    import_building,
                    "expected one liquid",
                ));
            };
            let inlets: BTreeSet<usize> = (tables.sewage_plants.values())
                .map(|plant| {
                    plant
                        .level_list
                        .iter()
                        .filter(|l| l.is_import_level)
                        .count()
                })
                .collect();
            let inlets = match inlets.iter().copied().collect::<Vec<_>>().as_slice() {
                [n] if *n > 0 => *n,
                _ => {
                    return Err(ExtractError::table(
                        "FactorySewageTreatPlantStoreTable",
                        building,
                        "expected every plant to have the same number of inlets, at least one",
                    ));
                }
            };
            let per_s = inlets as f64 * 1000.0 / import.ms_per_round;
            let facility = self.facility(building)?;
            let product = item_id(TABLE, building, &export.product_item_id)?;
            let inputs = vec![(sewage.clone(), export.count_cost)];
            let outputs = vec![(export.product_item_id.clone(), export.count_produce)];
            let id = format!("{facility}-{product}");
            let category = self.category_of_work(&inputs, &outputs, &id)?;
            let name = match (
                &self.facilities[&facility].name,
                self.item_name(&export.product_item_id),
            ) {
                (Some(f), Some(i)) => Some(format!("{f}({i})")),
                _ => None,
            };
            self.report.sewage_export = Some((inlets, export.count_cost / per_s));
            push_merging(
                drafts,
                Draft {
                    id,
                    name,
                    category,
                    time_s: export.count_cost / per_s,
                    inputs,
                    outputs,
                    facilities: vec![facility],
                    power_kw: None,
                    kind: None,
                    environment: None,
                    creates_environment: None,
                    projects: Vec::new(),
                    regions: None,
                    group: Group::Other,
                    sort_id: 0,
                },
            );
        }
        Ok(())
    }

    /// The Gas Dispersing Unit: per group, its gas at `consumeRate` a minute, nothing out, and the
    /// gas's environment spread. It runs in whole units (ADR 0015).
    ///
    /// `consumeRate` is taken as per minute, as the transmuting units' is (their 6 matches the
    /// 6/min of ADR 0013). Nothing in the tables says so for this unit.
    fn vaporizers(&mut self, drafts: &mut Vec<Draft>) -> Result<(), ExtractError> {
        let tables = self.tables;
        for (building, vaporizer) in &tables.vaporizers {
            for group in &vaporizer.groups {
                let mut draft = self.sink(
                    "FactoryVaporizerTable",
                    building,
                    &group.consumption.consume_item,
                    group.consumption.consume_rate,
                    60.0,
                )?;
                draft.creates_environment = Some(group.consumption.consume_item.clone());
                self.whole_units(&draft.facilities[0]);
                push_merging(drafts, draft);
            }
        }
        Ok(())
    }

    /// Marks a facility as running in whole units (ADR 0015). One with an activation input too
    /// would not load, which `run` checks.
    fn whole_units(&mut self, facility: &str) {
        self.facilities[facility].entry.whole_units = true;
    }

    /// The arrays (ADR 0015), from the tables v1.7.0 brought; nothing for a version without them.
    ///
    /// - An array facility per kind and tier, `<kind>_<tier>` (`magic_ming_3`), its speed the
    ///   tier's chance per pulse, no power, built where the forges are, and stacked from T1 arrays:
    ///   n of its own T1 at tier n, or, for a kind the combination table makes of others (an E of
    ///   an L and a T), n of each of theirs.
    /// - A recipe per conversion, under the game's craft ID at tier 1, on every tier of its kind,
    ///   lowest first: one item of each tile of the area in and out per pulse, at speed 1.
    /// - Per forge, a recipe for each choice of its arrays (`<forge>-<kind>-<kind>`), each taking
    ///   `consumeRate` of its gas a minute, as the Gas Dispersing Unit does, and projecting the
    ///   chosen T1 arrays. A forge runs in whole units.
    fn arrays(&mut self, drafts: &mut Vec<Draft>) -> Result<(), ExtractError> {
        const STAKES: &str = "FactoryFormationStakeTable";
        const CONVERSIONS: &str = "FactoryFormationTransferTable";
        const TIERS: &str = "FactoryFormationLevelTable";
        const COMBINATIONS: &str = "FactoryFormationCombinationTable";
        let tables = self.tables;
        if tables.forges.is_empty() && tables.array_conversions.is_empty() {
            return Ok(());
        }
        let tiles = area_tiles(tables)?;
        let pulse_s = match tables.constants.array_pulse_ms {
            Some(ms) if ms.is_finite() && ms > 0.0 => ms / 1000.0,
            other => {
                return Err(ExtractError::table(
                    "FactoryConst",
                    "facFormationEffectInterval",
                    format!("the arrays' pulse is {other:?}; expected a positive number of ms"),
                ));
            }
        };
        let regions = self.forge_regions()?;

        // The chance at each tier, by kind: every conversion of a kind must agree.
        let mut chances: IndexMap<&str, &[f64]> = IndexMap::new();
        for (key, conversion) in &tables.array_conversions {
            let kind = conversion.kind.as_str();
            if !tables.array_kinds.contains_key(kind) {
                return Err(ExtractError::table(
                    CONVERSIONS,
                    key,
                    format!("`{kind}` is no kind of FactoryFormationTable"),
                ));
            }
            let valid = |c: &f64| *c > 0.0 && *c <= 10_000.0;
            if conversion.chances.is_empty() || !conversion.chances.iter().all(valid) {
                return Err(ExtractError::table(
                    CONVERSIONS,
                    key,
                    format!(
                        "chances {:?}; expected one per tier, each above 0 and at most 10000",
                        conversion.chances
                    ),
                ));
            }
            match chances.get(kind) {
                None => {
                    chances.insert(kind, &conversion.chances);
                }
                Some(earlier) if *earlier == conversion.chances.as_slice() => {}
                Some(earlier) => {
                    return Err(ExtractError::table(
                        CONVERSIONS,
                        key,
                        format!(
                            "chances {:?}, where another conversion of `{kind}` has {earlier:?}; \
                             an array's tier has one speed",
                            conversion.chances
                        ),
                    ));
                }
            }
        }

        // An array facility per kind and tier.
        let mut tier_facilities: IndexMap<&str, Vec<String>> = IndexMap::new();
        let mut kind_names: HashMap<&str, String> = HashMap::new();
        for (&kind, &kind_chances) in &chances {
            let tiers = tables
                .array_tiers
                .get(kind)
                .ok_or_else(|| Self::missing(TIERS, kind, "the tiers of an array"))?;
            if tiers.tiers.len() != kind_chances.len() {
                return Err(ExtractError::table(
                    TIERS,
                    kind,
                    format!(
                        "{} tiers, where FactoryFormationTransferTable gives {} chances",
                        tiers.tiers.len(),
                        kind_chances.len()
                    ),
                ));
            }
            let sources: Vec<&str> = match tables
                .array_combinations
                .iter()
                .find(|(_, c)| c.target == kind)
            {
                Some((key, combination)) => {
                    for source in &combination.sources {
                        if !chances.contains_key(source.as_str()) {
                            return Err(ExtractError::table(
                                COMBINATIONS,
                                key,
                                format!("`{source}` is no kind of array with conversions"),
                            ));
                        }
                    }
                    combination.sources.iter().map(String::as_str).collect()
                }
                None => vec![kind],
            };
            let mut ids = Vec::new();
            for (tier, chance) in (1u32..).zip(kind_chances) {
                let row = (tiers.tiers.values())
                    .find(|row| row.tier == tier)
                    .ok_or_else(|| Self::missing(TIERS, kind, &format!("tier {tier}")))?;
                let id = format!("{kind}_{tier}");
                if self.facilities.contains_key(&id) {
                    return Err(ExtractError::table(
                        TIERS,
                        kind,
                        format!("array `{id}` has the ID of another facility"),
                    ));
                }
                let name = tables.text(&row.name).map(str::to_owned);
                if tier == 1
                    && let Some(name) = &name
                {
                    // "L-Array T1" is a T1 of the L-Array.
                    let kind_name = name.strip_suffix(" T1").unwrap_or(name);
                    kind_names.insert(kind, kind_name.to_owned());
                }
                let stack = (sources.iter())
                    .map(|source| (format!("{source}_1"), tier))
                    .collect();
                self.facilities.insert(
                    id.clone(),
                    FacilityDraft {
                        entry: FacilityEntry {
                            id: id.clone(),
                            speed: chance / 10_000.0,
                            power_kw: 0.0,
                            regions: regions.clone(),
                            activation: None,
                            whole_units: false,
                            stack,
                        },
                        name,
                        named_by_game: true,
                        // After every building.
                        order: usize::MAX,
                    },
                );
                ids.push(id);
            }
            tier_facilities.insert(kind, ids);
        }

        // A recipe per conversion, on every tier of its kind.
        for (key, conversion) in &tables.array_conversions {
            item_id(CONVERSIONS, key, &conversion.from_item_id)?;
            item_id(CONVERSIONS, key, &conversion.to_item_id)?;
            let id = format!("formation_{}_{}_1", conversion.kind, conversion.to_item_id);
            let name = (tables.crafts.get(&id))
                .and_then(|craft| tables.text(&craft.formula_desc))
                .map(str::to_owned);
            let inputs = vec![(conversion.from_item_id.clone(), tiles)];
            let outputs = vec![(conversion.to_item_id.clone(), tiles)];
            let category = self.category_of_work(&inputs, &outputs, &id)?;
            drafts.push(Draft {
                id,
                name,
                category,
                time_s: pulse_s,
                inputs,
                outputs,
                facilities: tier_facilities[conversion.kind.as_str()].clone(),
                power_kw: None,
                kind: None,
                environment: None,
                creates_environment: None,
                projects: Vec::new(),
                regions: None,
                group: Group::Other,
                sort_id: 0,
            });
        }

        // Per forge, a recipe for each choice of the arrays it projects.
        let mut forge_recipes = 0;
        for (building, forge) in &tables.forges {
            for kind in &forge.kinds {
                if !chances.contains_key(kind.as_str()) {
                    return Err(ExtractError::table(
                        STAKES,
                        building,
                        format!("`{kind}` is no kind of array with conversions"),
                    ));
                }
            }
            let facility = self.facility(building)?;
            self.whole_units(&facility);
            let gas = &forge.consumption.consume_item;
            item_id(STAKES, building, gas)?;
            let inputs = vec![(gas.clone(), forge.consumption.consume_rate)];
            for choice in choices(&forge.kinds, forge.areas.len()) {
                let id = format!("{facility}-{}", choice.join("-"));
                let mut projects: Vec<(String, u32)> = Vec::new();
                for kind in &choice {
                    let t1 = format!("{kind}_1");
                    match projects.iter_mut().find(|(id, _)| *id == t1) {
                        Some((_, n)) => *n += 1,
                        None => projects.push((t1, 1)),
                    }
                }
                let arrays: Option<Vec<&str>> = (choice.iter())
                    .map(|kind| kind_names.get(kind).map(String::as_str))
                    .collect();
                let name = match (&self.facilities[&facility].name, arrays) {
                    (Some(forge), Some(arrays)) => Some(format!("{forge}({})", arrays.join(" + "))),
                    _ => None,
                };
                let category = self.category_of_work(&inputs, &[], &id)?;
                drafts.push(Draft {
                    id,
                    name,
                    category,
                    time_s: 60.0,
                    inputs: inputs.clone(),
                    outputs: vec![],
                    facilities: vec![facility.clone()],
                    power_kw: None,
                    kind: None,
                    environment: None,
                    creates_environment: None,
                    projects,
                    regions: None,
                    group: Group::Other,
                    sort_id: 0,
                });
                forge_recipes += 1;
            }
        }

        self.report.arrays = Some(ArraysReport {
            kinds: tier_facilities.len(),
            arrays: tier_facilities.values().map(Vec::len).sum(),
            conversions: tables.array_conversions.len(),
            forge_recipes,
            tiles,
            pulse_s,
            crafts_taken_off: std::mem::take(&mut self.array_crafts).into_iter().collect(),
        });
        Ok(())
    }

    /// The regions the forges can be built in, which their arrays are in.
    fn forge_regions(&self) -> Result<Vec<String>, ExtractError> {
        let mut regions = BTreeSet::new();
        for building in self.tables.forges.keys() {
            let row = (self.tables.buildings.get(building)).ok_or_else(|| {
                Self::missing("FactoryBuildingTable", building, "a Forge of Luminance")
            })?;
            let domains = if row.place_domains.is_empty() {
                &row.recommend_domains
            } else {
                &row.place_domains
            };
            for domain in domains {
                regions.insert(region_of("FactoryBuildingTable", building, domain)?);
            }
        }
        Ok(ordered_regions(&regions))
    }

    /// Settlement trades: per region, every item its outposts take at their highest level, for
    /// the coupons they pay.
    fn trades(&mut self, drafts: &mut Vec<Draft>) -> Result<(), ExtractError> {
        const TABLE: &str = "SettlementBasicDataTable";
        let tables = self.tables;
        for (domain_id, domain) in &tables.domains {
            let region = region_of("DomainDataTable", domain_id, domain_id)?;
            let coupon = &domain.domain_gold_item_id;
            item_id("DomainDataTable", domain_id, coupon)?;
            let mut prices: IndexMap<&str, Vec<f64>> = IndexMap::new();
            let settlements =
                (tables.settlements.iter()).filter(|(_, s)| s.domain_id == *domain_id);
            for (settlement_id, settlement) in settlements {
                let top = (settlement.settlement_level_map.iter())
                    .map(|(level, row)| level.parse::<u32>().map(|n| (n, row)))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| {
                        ExtractError::table(TABLE, settlement_id, "a level that is not a number")
                    })?
                    .into_iter()
                    .max_by_key(|(n, _)| *n)
                    .map(|(_, row)| row);
                let Some(top) = top else { continue };
                for (item, trade) in &top.settlement_trade_item_map {
                    prices
                        .entry(item)
                        .or_default()
                        .push(trade.reward_money_count);
                }
            }
            if prices.is_empty() {
                continue;
            }
            let facility = format!("settlement-{region}_coupon");
            if !self.facilities.contains_key(&facility) {
                self.facilities.insert(
                    facility.clone(),
                    FacilityDraft {
                        entry: FacilityEntry {
                            id: facility.clone(),
                            speed: 1.0,
                            power_kw: 0.0,
                            regions: vec![region.to_owned()],
                            activation: None,
                            whole_units: false,
                            stack: IndexMap::new(),
                        },
                        name: (tables.text(&domain.domain_name))
                            .map(|n| format!("Outpost Supplies({n})")),
                        named_by_game: false,
                        // After every building.
                        order: usize::MAX,
                    },
                );
            }
            let coupon_name = self.item_name(coupon);
            for (item, offers) in prices {
                let kept = offers.iter().copied().fold(f64::MIN, f64::max);
                if offers.iter().any(|&p| p != kept) {
                    self.report.price_conflicts.push(PriceConflict {
                        region: region.to_owned(),
                        item: item_id(TABLE, item, item)?,
                        offers: offers.clone(),
                        kept,
                    });
                }
                let id = format!("{region}_coupon-{}", item_id(TABLE, item, item)?);
                let name = match (&coupon_name, self.item_name(item)) {
                    (Some(c), Some(i)) => Some(format!("{c}({i})")),
                    _ => None,
                };
                drafts.push(Draft {
                    id,
                    name,
                    category: SETTLEMENT.to_owned(),
                    time_s: TRADE_TIME_S,
                    inputs: vec![((*item).to_owned(), 1.0)],
                    outputs: vec![(coupon.clone(), kept)],
                    facilities: vec![facility.clone()],
                    power_kw: None,
                    kind: Some(RecipeKind::Trade),
                    environment: None,
                    creates_environment: None,
                    projects: Vec::new(),
                    regions: Some(vec![region.to_owned()]),
                    group: Group::Trade,
                    sort_id: 0,
                });
            }
        }
        Ok(())
    }

    /// Marks the crafts that fill a container with a fluid and the ones that empty it again, and
    /// names them `Packaged <fluid> (<container>)` and `Unpackage <fluid> (<container>)`.
    ///
    /// The game's names don't tell them apart: every emptying of an Amethyst Bottle is "Amethyst
    /// Bottle Production", whatever was in it, and a filling is "Clean Water (Bottled)" whichever
    /// bottle it fills. A filling is a craft of one solid and one fluid into one item that some
    /// craft turns back into those two; mixing a solid into a liquid for good (a seed and water
    /// into a plant) is not one.
    fn packaging(&mut self, drafts: &mut [Draft]) {
        let tables = self.tables;
        let fluid = |item: &str| {
            let phase = tables.factory_items.get(item).map(|row| row.phase_type);
            matches!(phase, Some(2 | 4))
        };
        for fill in 0..drafts.len() {
            let draft = &drafts[fill];
            let ([(a, _), (b, _)], [(filled, _)]) =
                (draft.inputs.as_slice(), draft.outputs.as_slice())
            else {
                continue;
            };
            let (container, contents) = match (fluid(a), fluid(b)) {
                (false, true) => (a.clone(), b.clone()),
                (true, false) => (b.clone(), a.clone()),
                _ => continue,
            };
            let filled = filled.clone();
            let empty = drafts.iter().position(|d| {
                matches!(d.inputs.as_slice(), [(item, _)] if *item == filled)
                    && d.outputs.len() == 2
                    && has_item(&d.outputs, &container)
                    && has_item(&d.outputs, &contents)
            });
            let Some(empty) = empty else { continue };
            let names = (self.item_name(&contents), self.item_name(&container));
            for (index, group, verb) in [
                (fill, Group::Filling, "Packaged"),
                (empty, Group::Emptying, "Unpackage"),
            ] {
                let draft = &mut drafts[index];
                draft.group = group;
                if let (Some(contents), Some(container)) = &names {
                    draft.name = Some(format!("{verb} {contents} ({container})"));
                    self.renamed.insert(draft.id.clone());
                }
            }
        }
    }

    /// Tells apart recipes the game gives one name: "Carbon Powder Production" is six recipes,
    /// from five plant powders and from Carbon. Each takes, in brackets, the name of its first
    /// input that the others of that name don't all take; where they all take the same items,
    /// the gas whose environment it needs. One of them may have neither, and keeps the plain
    /// name. The new names take the place of the names table's. Names still shared after that
    /// go in the report.
    fn tell_apart(&mut self, drafts: &mut [Draft], names: &NamesFile) {
        // An item's name as the names table has it, which is what the app shows.
        let shown = |game_id: &str| {
            let id = game_id.strip_prefix("item_").unwrap_or(game_id);
            names.items.get(id).cloned()
        };
        let mut alike: IndexMap<String, Vec<usize>> = IndexMap::new();
        for (index, draft) in drafts.iter().enumerate() {
            if let Some(name) = &draft.name {
                alike.entry(name.clone()).or_default().push(index);
            }
        }
        for (name, group) in alike.iter().filter(|(_, group)| group.len() > 1) {
            let takes = |index: usize, item: &str| has_item(&drafts[index].inputs, item);
            let by_input: Vec<Option<String>> = (group.iter())
                .map(|&index| {
                    (drafts[index].inputs.iter())
                        .find(|(item, _)| !group.iter().all(|&other| takes(other, item)))
                        .and_then(|(item, _)| shown(item))
                })
                .collect();
            let by_environment: Vec<Option<String>> = (group.iter())
                .map(|&index| drafts[index].environment.as_deref().and_then(shown))
                .collect();
            let Some(tags) = [by_input, by_environment]
                .into_iter()
                .find(|tags| all_differ(tags))
            else {
                continue;
            };
            for (&index, tag) in group.iter().zip(tags) {
                if let Some(tag) = tag {
                    drafts[index].name = Some(format!("{name} ({tag})"));
                    self.renamed.insert(drafts[index].id.clone());
                }
            }
        }
        let mut shared: IndexMap<&str, Vec<&str>> = IndexMap::new();
        for draft in drafts.iter() {
            if let Some(name) = &draft.name {
                shared.entry(name).or_default().push(&draft.id);
            }
        }
        self.report.shared_names = shared
            .into_iter()
            .filter(|(_, recipes)| recipes.len() > 1)
            .map(|(name, recipes)| SharedName {
                name: name.to_owned(),
                recipes: recipes.into_iter().map(str::to_owned).collect(),
            })
            .collect();
    }

    /// The regions a recipe exists in: those every item that is limited to some regions shares.
    fn recipe_regions(&mut self, draft: &Draft) -> Result<Vec<String>, ExtractError> {
        if let Some(regions) = &draft.regions {
            return Ok(regions.clone());
        }
        let mut shared: Option<BTreeSet<&'static str>> = None;
        let items = (draft.inputs.iter().chain(&draft.outputs)).map(|(item, _)| item);
        for item in items {
            let Some(row) = self.tables.factory_items.get(item) else {
                continue;
            };
            if row.show_in_hub_domain_ids.is_empty() {
                continue;
            }
            let mut regions = BTreeSet::new();
            for domain in &row.show_in_hub_domain_ids {
                regions.insert(region_of("FactoryItemTable", item, domain)?);
            }
            shared = Some(match shared {
                None => regions,
                Some(s) => s.intersection(&regions).copied().collect(),
            });
        }
        match shared {
            Some(s) if s.is_empty() => {
                self.report.regionless.push(draft.id.clone());
                Ok(vec![])
            }
            Some(s) => Ok(ordered_regions(&s)),
            None => Ok(vec![]),
        }
    }

    fn finish(
        mut self,
        mut drafts: Vec<Draft>,
        previous: Option<&DataFile>,
        names: &mut NamesFile,
        version: &str,
    ) -> Result<Extraction, ExtractError> {
        let tables = self.tables;
        // The game's order: mining, crafts, the other buildings, fillings and emptyings, trades.
        // Crafts by facility in the game's building table order, then the game's `sortId`, then
        // ID; the rest as they were gathered (rigs, then the gas extractor, then pumps; a stable
        // sort keeps that order). A previous version's order is laid over it below.
        drafts.sort_by(|a, b| {
            let key = |d: &Draft| {
                if d.group.is_craft() {
                    let facility = self.facilities[&d.facilities[0]].order;
                    (d.group, facility, d.sort_id, d.id.clone())
                } else {
                    (d.group, 0, 0, String::new())
                }
            };
            key(a).cmp(&key(b))
        });

        // Items, in the order the recipes first name them.
        let mut item_ids: IndexMap<String, ()> = IndexMap::new();
        for draft in &drafts {
            let items = (draft.inputs.iter().chain(&draft.outputs)).map(|(item, _)| item);
            for item in items
                .chain(&draft.environment)
                .chain(&draft.creates_environment)
            {
                item_ids.insert(item.clone(), ());
            }
        }
        let mut facility_ids: IndexMap<String, ()> = IndexMap::new();
        for draft in &drafts {
            for facility in &draft.facilities {
                facility_ids.insert(facility.clone(), ());
            }
        }
        // And the T1 arrays that recipes project and arrays are stacked from, should no recipe
        // run on one.
        let named: Vec<String> = (drafts.iter().flat_map(|d| &d.projects))
            .map(|(facility, _)| facility.clone())
            .chain(
                (facility_ids.keys())
                    .flat_map(|id| self.facilities[id].entry.stack.keys().cloned()),
            )
            .collect();
        for facility in named {
            if !self.facilities.contains_key(&facility) {
                return Err(ExtractError::table(
                    "FactoryFormationLevelTable",
                    &facility,
                    "a projection no array of the tables is",
                ));
            }
            facility_ids.insert(facility, ());
        }
        for facility in facility_ids.keys() {
            if let Some(activation) = &self.facilities[facility].entry.activation {
                item_ids.insert(format!("item_{}", activation.item), ());
            }
        }

        let colors: HashMap<&str, &str> = previous
            .map(|p| {
                (p.items.iter())
                    .filter_map(|i| Some((i.id.as_str(), i.color.as_deref()?)))
                    .collect()
            })
            .unwrap_or_default();
        let item_names = self.item_names(item_ids.keys(), &drafts);
        let mut items = Vec::new();
        for game_id in item_ids.keys() {
            let id = item_id("FactoryItemTable", game_id, game_id)?;
            let phase = match tables.factory_items.get(game_id).map(|r| r.phase_type) {
                // Coupons have no factory row: they never touch a belt.
                None | Some(1) => Phase::Solid,
                Some(2) => Phase::Liquid,
                Some(4) => Phase::Gas,
                Some(n) => {
                    return Err(ExtractError::table(
                        "FactoryItemTable",
                        game_id,
                        format!("unknown phaseType {n}"),
                    ));
                }
            };
            let color = colors.get(id.as_str()).map(|c| (*c).to_owned());
            if let Some(name) = item_names.get(game_id.as_str()) {
                self.name(names, NameKind::Item, &id, name);
            } else {
                self.report.missing_names.push(format!("item {id}"));
            }
            items.push(ItemEntry { id, phase, color });
        }

        let mut facilities = Vec::new();
        for id in facility_ids.keys() {
            let facility = &self.facilities[id];
            let (entry, name) = (facility.entry.clone(), facility.name.clone());
            match name {
                Some(name) if facility.named_by_game => {
                    let (added, renamed) = names.set(NameKind::Item, id, &name);
                    self.report.names_added += usize::from(added);
                    self.report.names_changed.extend(renamed);
                }
                Some(name) => self.name(names, NameKind::Item, id, &name),
                None => self.report.missing_names.push(format!("facility {id}")),
            }
            facilities.push(entry);
        }

        // After the items, whose names in the table it uses.
        self.tell_apart(&mut drafts, names);

        let mut used_categories = BTreeSet::new();
        let mut recipes = Vec::new();
        for draft in &drafts {
            let regions = self.recipe_regions(draft)?;
            let quantities =
                |list: &[(String, f64)]| -> Result<IndexMap<String, f64>, ExtractError> {
                    list.iter()
                        .map(|(item, qty)| Ok((item_id("FactoryItemTable", item, item)?, *qty)))
                        .collect()
                };
            match &draft.name {
                Some(name) if self.renamed.contains(&draft.id) => {
                    let (added, renamed) = names.set(NameKind::Recipe, &draft.id, name);
                    self.report.names_added += usize::from(added);
                    self.report.names_changed.extend(renamed);
                }
                Some(name) => self.name(names, NameKind::Recipe, &draft.id, name),
                None => self
                    .report
                    .missing_names
                    .push(format!("recipe {}", draft.id)),
            }
            used_categories.insert(draft.category.clone());
            recipes.push(RecipeEntry {
                id: draft.id.clone(),
                category: draft.category.clone(),
                time_s: draft.time_s,
                inputs: quantities(&draft.inputs)?,
                outputs: quantities(&draft.outputs)?,
                facilities: draft.facilities.clone(),
                power_kw: draft.power_kw,
                kind: draft.kind,
                hidden: false,
                regions,
                environment: match &draft.environment {
                    Some(gas) => Some(item_id("FactoryVaporizerTable", gas, gas)?),
                    None => None,
                },
                creates_environment: match &draft.creates_environment {
                    Some(gas) => Some(item_id("FactoryVaporizerTable", gas, gas)?),
                    None => None,
                },
                projects: draft.projects.iter().cloned().collect(),
            });
        }

        let categories = self.categories(&used_categories, names)?;
        let regions = self.regions(names)?;
        let mut data = DataFile {
            format: FORMAT,
            version: Some(version.to_owned()),
            source: Some(format!(
                "Extracted by aic-extract from the game client's tables ({version})."
            )),
            belt_per_min: belt_per_min(tables)?,
            pipe_per_min: pipe_per_min(tables)?,
            regions,
            categories,
            items,
            facilities,
            recipes,
            aliases: Aliases::default(),
        };
        if let Some(previous) = previous {
            data.aliases = aliases(previous, &data, &mut self.report);
            let groups: HashMap<&str, Group> =
                (drafts.iter().map(|d| (d.id.as_str(), d.group))).collect();
            data.recipes = ordered_like(previous, &data.aliases, data.recipes, &groups);
        }
        Ok(Extraction {
            data,
            report: self.report,
        })
    }

    /// The items' names. The game calls a filled bottle or canister by its empty container's name
    /// and shows what is in it apart, so where items share a name, one made by filling a
    /// container of that name is called `<container>(<contents>)`, as in `Ferrium
    /// Bottle(Jincao Solution)`.
    fn item_names<'d>(
        &self,
        ids: impl Iterator<Item = &'d String>,
        drafts: &[Draft],
    ) -> HashMap<&'d str, String> {
        let plain: Vec<(&str, String)> = ids
            .filter_map(|id| Some((id.as_str(), self.item_name(id)?)))
            .collect();
        let mut sharing: HashMap<&str, usize> = HashMap::new();
        for (_, name) in &plain {
            *sharing.entry(name.as_str()).or_default() += 1;
        }
        let name_of = |id: &str| plain.iter().find(|(i, _)| *i == id).map(|(_, n)| n);
        let filled = |id: &str, name: &str| -> Option<String> {
            drafts.iter().find_map(|draft| {
                let [(output, _)] = draft.outputs.as_slice() else {
                    return None;
                };
                let [(a, _), (b, _)] = draft.inputs.as_slice() else {
                    return None;
                };
                if output != id {
                    return None;
                }
                let (container, contents) = if name_of(a)? == name { (a, b) } else { (b, a) };
                (name_of(container)? == name && container != id)
                    .then(|| Some(format!("{name}({})", name_of(contents)?)))
                    .flatten()
            })
        };
        plain
            .iter()
            .map(|(id, name)| {
                let name = match sharing[name.as_str()] {
                    1 => name.clone(),
                    _ => filled(id, name).unwrap_or_else(|| name.clone()),
                };
                (*id, name)
            })
            .collect()
    }

    fn name(&mut self, names: &mut NamesFile, kind: NameKind, id: &str, name: &str) {
        let (added, difference) = names.add(kind, id, name);
        if added {
            self.report.names_added += 1;
        }
        self.report.name_differences.extend(difference);
    }

    /// The `type_<n>` categories in `ItemShowingTypeTable` order, then power, then trades: only
    /// those a recipe has.
    fn categories(
        &mut self,
        used: &BTreeSet<String>,
        names: &mut NamesFile,
    ) -> Result<Vec<String>, ExtractError> {
        let mut types: Vec<(i64, String, Option<String>)> = Vec::new();
        for category in used {
            let Some(n) = category.strip_prefix("type_") else {
                continue;
            };
            let row = self.tables.showing_types.get(n).ok_or_else(|| {
                Self::missing("ItemShowingTypeTable", n, &format!("category `{category}`"))
            })?;
            let name = self.tables.text(&row.name).map(str::to_owned);
            types.push((row.sort_id, category.clone(), name));
        }
        types.sort();
        let mut categories = Vec::new();
        for (_, id, name) in types {
            match name {
                Some(name) => self.name(names, NameKind::Category, &id, &name),
                None => self.report.missing_names.push(format!("category {id}")),
            }
            categories.push(id);
        }
        for (id, name) in [(GEN_POWER, "Power"), (SETTLEMENT, "Outpost")] {
            if used.contains(id) {
                self.name(names, NameKind::Category, id, name);
                categories.push(id.to_owned());
            }
        }
        Ok(categories)
    }

    /// The regions, in the order of `REGIONS`, named by their domains.
    fn regions(&mut self, names: &mut NamesFile) -> Result<Vec<String>, ExtractError> {
        let mut regions = Vec::new();
        for (domain_id, region) in REGIONS {
            let domain = self.tables.domains.get(domain_id).ok_or_else(|| {
                Self::missing("DomainDataTable", domain_id, &format!("region `{region}`"))
            })?;
            match self.tables.text(&domain.domain_name) {
                Some(name) => {
                    let name = name.to_owned();
                    self.name(names, NameKind::Region, region, &name);
                }
                None => self.report.missing_names.push(format!("region {region}")),
            }
            regions.push(region.to_owned());
        }
        Ok(regions)
    }
}

/// The craft IDs under which the game lists the arrays' conversions, one per tier
/// (`formation_<kind>_<to item>_<tier>`, on the Forge of Equilibrium). The extractor writes each
/// conversion once, on the arrays, instead.
fn array_craft_ids(tables: &Tables) -> HashSet<String> {
    (tables.array_conversions.values())
        .flat_map(|conversion| {
            (1..=conversion.chances.len()).map(move |tier| {
                format!(
                    "formation_{}_{}_{tier}",
                    conversion.kind, conversion.to_item_id
                )
            })
        })
        .collect()
}

/// The tiles of an array's area (width × depth): one item on a belt on each is what an array
/// converts at most per pulse. Every area of every forge must be the same size.
fn area_tiles(tables: &Tables) -> Result<f64, ExtractError> {
    const STAKES: &str = "FactoryFormationStakeTable";
    let mut tiles = None;
    for (building, forge) in &tables.forges {
        if forge.areas.is_empty() {
            return Err(ExtractError::table(STAKES, building, "projects no area"));
        }
        for area in &forge.areas {
            let here = area.width * area.depth;
            match tiles {
                None => tiles = Some(here),
                Some(seen) if seen == here => {}
                Some(seen) => {
                    return Err(ExtractError::table(
                        STAKES,
                        building,
                        format!(
                            "an area of {here} tiles, where another has {seen}; the extractor \
                             takes every array to be the same size"
                        ),
                    ));
                }
            }
        }
    }
    match tiles {
        Some(tiles) if tiles > 0.0 => Ok(tiles),
        _ => Err(ExtractError::table(
            STAKES,
            "",
            "no forge with an area, so the arrays of FactoryFormationTransferTable have no size",
        )),
    }
}

/// Every choice of `count` arrays from `kinds`, the same kind more than once allowed and the
/// order not mattering, in the order of `kinds`: for L and T, two at a time, L + L, L + T, T + T.
fn choices(kinds: &[String], count: usize) -> Vec<Vec<&str>> {
    if count == 0 {
        return vec![Vec::new()];
    }
    let mut all = Vec::new();
    for (i, kind) in kinds.iter().enumerate() {
        for rest in choices(&kinds[i..], count - 1) {
            let mut choice = vec![kind.as_str()];
            choice.extend(rest);
            all.push(choice);
        }
    }
    all
}

fn has_item(list: &[(String, f64)], item: &str) -> bool {
    list.iter().any(|(i, _)| i == item)
}

/// Whether no two of `tags` are the same; two that are missing count as the same.
fn all_differ(tags: &[Option<String>]) -> bool {
    (tags.iter().enumerate()).all(|(i, tag)| !tags[..i].contains(tag))
}

/// Regions in the order of `REGIONS`; empty when they are all of them.
fn ordered_regions(regions: &BTreeSet<&str>) -> Vec<String> {
    if REGIONS.iter().all(|(_, r)| regions.contains(r)) {
        return vec![];
    }
    (REGIONS.iter())
        .filter(|(_, r)| regions.contains(r))
        .map(|(_, r)| (*r).to_owned())
        .collect()
}

/// Items per minute of the belt: one item per round.
fn belt_per_min(tables: &Tables) -> Result<f64, ExtractError> {
    let (_, belt) = (tables.belts.first())
        .ok_or_else(|| ExtractError::table("FactoryGridBeltTable", "", "no belt"))?;
    Ok(60_000.0 / belt.belt_data.ms_per_round)
}

/// Items per minute of the pipe: one unit per round.
fn pipe_per_min(tables: &Tables) -> Result<f64, ExtractError> {
    let (_, pipe) = (tables.pipes.first())
        .ok_or_else(|| ExtractError::table("FactoryLiquidPipeTable", "", "no pipe"))?;
    Ok(60_000.0 / pipe.pipe_data.ms_per_round)
}

/// The recipes in the previous version's order, so the recipe browser keeps the order a player
/// knows from one version to the next (the first version's was factoriolab's, which follows each
/// raw resource through what is made of it).
///
/// A recipe the previous version had, under the same ID or one that is now its alias, goes where
/// it was. One it lacked goes right after the last recipe placed so far whose main product is the
/// same item, or, when it makes nothing, the item it takes, and after the new ones already placed
/// by that item, so they stay in the game's order; with none, after them all, in the game's order. Fillings and emptyings, then trades, stay at the end wherever they were.
fn ordered_like(
    previous: &DataFile,
    aliases: &Aliases,
    recipes: Vec<RecipeEntry>,
    groups: &HashMap<&str, Group>,
) -> Vec<RecipeEntry> {
    let ids: HashSet<&str> = recipes.iter().map(|r| r.id.as_str()).collect();
    let mut rank: HashMap<String, usize> = HashMap::new();
    for (place, old) in previous.recipes.iter().enumerate() {
        let now = if ids.contains(old.id.as_str()) {
            Some(&old.id)
        } else {
            aliases.recipes.get(&old.id)
        };
        if let Some(id) = now {
            rank.entry(id.clone()).or_insert(place);
        }
    }
    let (mut packaging, mut trades, mut known, mut new) = (vec![], vec![], vec![], vec![]);
    for recipe in recipes {
        match groups.get(recipe.id.as_str()) {
            Some(Group::Trade) => trades.push(recipe),
            Some(group) if group.is_packaging() => packaging.push(recipe),
            _ if rank.contains_key(&recipe.id) => known.push(recipe),
            _ => new.push(recipe),
        }
    }
    known.sort_by_key(|recipe| rank[&recipe.id]);
    // The item a new recipe is placed by: its main product, or what it takes if it makes nothing.
    let anchor = |recipe: &RecipeEntry| -> Option<String> {
        (recipe.outputs.keys().next())
            .or(recipe.inputs.keys().next())
            .cloned()
    };
    // New recipes placed so far, by ID, with the item each was placed by: the next one placed by
    // the same item goes after them, so they keep the game's order among themselves.
    let mut placed_new: HashMap<String, Option<String>> = HashMap::new();
    let mut unplaced = Vec::new();
    for recipe in new {
        let item = anchor(&recipe);
        let after = known.iter().rposition(|placed| {
            placed.outputs.keys().next() == item.as_ref()
                || placed_new.get(&placed.id).is_some_and(|by| *by == item)
        });
        match after {
            Some(at) => {
                placed_new.insert(recipe.id.clone(), item);
                known.insert(at + 1, recipe);
            }
            None => unplaced.push(recipe),
        }
    }
    known.extend(unplaced);
    known.extend(packaging);
    known.extend(trades);
    known
}

/// Aliases from `previous`'s IDs, and from the older IDs its own aliases map, to `data`'s.
///
/// Items: [`ITEM_ALIASES`], where the old ID is gone and the new one exists. Recipes: for each old
/// recipe whose ID is gone, the one new recipe that runs on every facility the old one listed and
/// takes and makes the same items (old ones through the item aliases) at the same rates per
/// second, as the files list them (activation inputs apart). None or several: no alias, and the
/// report says so. `previous`'s own aliases come first, carried forward: re-pointed where their
/// target got an alias in this run, and left out and reported where it is gone.
pub fn aliases(previous: &DataFile, data: &DataFile, report: &mut Report) -> Aliases {
    let has_item = |file: &DataFile, id: &str| file.items.iter().any(|i| i.id == id);
    let fresh_items: IndexMap<String, String> = ITEM_ALIASES
        .iter()
        .filter(|(old, new)| has_item(previous, old) && !has_item(data, old) && has_item(data, new))
        .map(|(old, new)| ((*old).to_owned(), (*new).to_owned()))
        .collect();
    let items = carried(
        &previous.aliases.items,
        fresh_items,
        |id| has_item(data, id),
        AliasKind::Item,
        report,
    );
    let renamed = |id: &str| items.get(id).map_or(id, String::as_str).to_owned();
    let rates = |list: &IndexMap<String, f64>, time_s: f64, rename: bool| -> Vec<(String, f64)> {
        let mut rates: Vec<(String, f64)> = list
            .iter()
            .map(|(item, qty)| {
                let item = if rename { renamed(item) } else { item.clone() };
                (item, qty / time_s)
            })
            .collect();
        rates.sort_by(|a, b| a.0.cmp(&b.0));
        rates
    };
    let same = |a: &[(String, f64)], b: &[(String, f64)]| {
        a.len() == b.len()
            && a.iter().zip(b).all(|((ia, ra), (ib, rb))| {
                ia == ib && (ra - rb).abs() <= 1e-9 * ra.abs().max(rb.abs())
            })
    };
    let mut recipes = IndexMap::new();
    for old in &previous.recipes {
        if data.recipes.iter().any(|r| r.id == old.id) {
            continue;
        }
        let old_in = rates(&old.inputs, old.time_s, true);
        let old_out = rates(&old.outputs, old.time_s, true);
        let matches: Vec<&RecipeEntry> = (data.recipes.iter())
            .filter(|new| old.facilities.iter().all(|f| new.facilities.contains(f)))
            .filter(|new| {
                same(&old_in, &rates(&new.inputs, new.time_s, false))
                    && same(&old_out, &rates(&new.outputs, new.time_s, false))
            })
            .collect();
        match matches.as_slice() {
            [one] => {
                recipes.insert(old.id.clone(), one.id.clone());
            }
            [] => report.unaliased.push(Unaliased {
                id: old.id.clone(),
                reason: "no recipe on its facilities makes the same at the same rates".to_owned(),
            }),
            several => report.unaliased.push(Unaliased {
                id: old.id.clone(),
                reason: format!(
                    "several recipes match: {}",
                    several
                        .iter()
                        .map(|r| r.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }),
        }
    }
    let recipes = carried(
        &previous.aliases.recipes,
        recipes,
        |id| data.recipes.iter().any(|r| r.id == id),
        AliasKind::Recipe,
        report,
    );
    Aliases { items, recipes }
}

/// The previous version's own aliases (`older`), then this run's (`fresh`), each pointing at an
/// ID this version has.
///
/// An old ID this version has itself needs no alias. One whose target is still here keeps it; one
/// whose target got an alias this run follows it, so no alias leads to another. The rest stand for
/// something gone, with nothing in its place: left out, and the report says so.
fn carried(
    older: &IndexMap<String, String>,
    fresh: IndexMap<String, String>,
    has: impl Fn(&str) -> bool,
    kind: AliasKind,
    report: &mut Report,
) -> IndexMap<String, String> {
    let mut aliases = IndexMap::new();
    for (old, target) in older {
        if has(old) {
            continue;
        }
        let now = if has(target) {
            Some(target)
        } else {
            fresh.get(target)
        };
        match now {
            Some(now) => {
                aliases.insert(old.clone(), now.clone());
            }
            None => report.dropped_aliases.push(DroppedAlias {
                kind,
                id: old.clone(),
                target: target.clone(),
            }),
        }
    }
    aliases.extend(fresh);
    aliases
}
