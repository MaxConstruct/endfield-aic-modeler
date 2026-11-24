//! Turns a data file, its overlay and a names table into a checked `Catalog`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Deserialize;

use crate::catalog::{
    ActivationInput, Catalog, Category, Facility, Item, LoadWarning, NameKind, Recipe, Region,
};
use crate::error::LoadError;
use crate::file::{
    Aliases, DataFile, Entry, FacilityEntry, ItemEntry, Overlay, OverlayList, RecipeEntry,
    RecipeKind, from_json,
};
use crate::ids::{CategoryId, FacilityId, ItemId, RecipeId, RegionId};
use crate::version::Names;

type Items = IndexMap<ItemId, Item>;
type Facilities = IndexMap<FacilityId, Facility>;
type Regions = IndexMap<RegionId, Region>;

/// The file names that errors report.
pub(crate) struct Sources<'a> {
    pub data: &'a Path,
    pub overlay: &'a Path,
    pub names: &'a Path,
}

/// A names table, `crates/aic-editor/i18n/names/<lang>.json`: display names keyed by the data's
/// IDs, the same for every data version. Facilities are named under `items`. Other keys (the
/// table's own `name` and `version`) are ignored.
#[derive(Debug, Default, Deserialize)]
struct NamesTable {
    #[serde(default)]
    categories: HashMap<String, String>,
    #[serde(default)]
    items: HashMap<String, String>,
    #[serde(default)]
    recipes: HashMap<String, String>,
    #[serde(default)]
    locations: HashMap<String, String>,
}

impl Catalog {
    /// Parses a catalog from the contents of a data version's file, named by `names`. Errors call
    /// the data `data.json`.
    ///
    /// The version is what the data declares; `aic_data::load` replaces it with the file's name.
    pub fn from_bytes(data_json: &[u8], names: Names<'_>) -> Result<Catalog, LoadError> {
        Self::from_bytes_with_overlay(data_json, None, names)
    }

    /// Like [`Catalog::from_bytes`], with the contents of an overlay merged on top of the data
    /// first. Errors call it `overlay.json`.
    pub fn from_bytes_with_overlay(
        data_json: &[u8],
        overlay_json: Option<&[u8]>,
        names: Names<'_>,
    ) -> Result<Catalog, LoadError> {
        let sources = Sources {
            data: Path::new("data.json"),
            overlay: Path::new("overlay.json"),
            names: Path::new(names.file),
        };
        parse(data_json, overlay_json, names.json, &sources)
    }
}

pub(crate) fn parse(
    data_json: &[u8],
    overlay_json: Option<&[u8]>,
    names_json: &[u8],
    sources: &Sources<'_>,
) -> Result<Catalog, LoadError> {
    let mut data = DataFile::from_json(data_json, sources.data)?;
    if let Some(overlay) = overlay_json {
        let overlay = Overlay::from_json(overlay, sources.overlay)?;
        apply_overlay(&mut data, overlay, sources.overlay)?;
    }
    let names: NamesTable = from_json(names_json, sources.names)?;
    let mut builder = Builder {
        file: sources.data.to_path_buf(),
        names,
        warnings: Vec::new(),
    };
    let mut catalog = builder.build(data)?;
    catalog.warnings = builder.warnings;
    Ok(catalog)
}

/// Merges an overlay into the data before it is checked, so overlay entries get the same checks as
/// the data's own.
fn apply_overlay(data: &mut DataFile, overlay: Overlay, file: &Path) -> Result<(), LoadError> {
    let invalid = |path: String, message: String| LoadError::Invalid {
        file: file.to_path_buf(),
        path,
        message,
    };
    merge(&mut data.items, overlay.items, "items", &invalid)?;
    merge(
        &mut data.facilities,
        overlay.facilities,
        "facilities",
        &invalid,
    )?;
    merge(&mut data.recipes, overlay.recipes, "recipes", &invalid)?;
    Ok(())
}

fn merge<T: Entry>(
    entries: &mut Vec<T>,
    changes: OverlayList<T>,
    key: &str,
    invalid: &dyn Fn(String, String) -> LoadError,
) -> Result<(), LoadError> {
    let position = |entries: &[T], id: &str| entries.iter().position(|e| e.id() == id);
    for (i, id) in changes.remove.iter().enumerate() {
        let at = position(entries, id)
            .ok_or_else(|| invalid(format!("{key}.remove[{i}]"), format!("no `{id}` to remove")))?;
        entries.remove(at);
    }
    for (i, entry) in changes.replace.into_iter().enumerate() {
        let id = entry.id().to_owned();
        let at = position(entries, &id).ok_or_else(|| {
            invalid(
                format!("{key}.replace[{i}].id"),
                format!("no `{id}` to replace"),
            )
        })?;
        entries[at] = entry;
    }
    for (i, entry) in changes.add.into_iter().enumerate() {
        if position(entries, entry.id()).is_some() {
            return Err(invalid(
                format!("{key}.add[{i}].id"),
                format!("`{}` already exists; use `replace`", entry.id()),
            ));
        }
        entries.push(entry);
    }
    Ok(())
}

struct Builder {
    file: PathBuf,
    names: NamesTable,
    warnings: Vec<LoadWarning>,
}

impl Builder {
    fn build(&mut self, data: DataFile) -> Result<Catalog, LoadError> {
        let belt_per_min =
            check_positive(data.belt_per_min).map_err(|m| self.invalid("belt_per_min", m))?;
        let pipe_per_min =
            check_positive(data.pipe_per_min).map_err(|m| self.invalid("pipe_per_min", m))?;
        let regions = self.regions(&data.regions)?;
        let categories = self.categories(&data.categories);
        let items = self.items(&data.items)?;
        let facilities = self.facilities(&data.facilities, &items, &regions)?;
        let recipes = self.recipes(&data.recipes, &items, &facilities, &regions)?;
        let (item_aliases, recipe_aliases) = self.aliases(&data.aliases, &items, &recipes)?;
        Ok(Catalog {
            items,
            facilities,
            recipes,
            regions,
            categories,
            belt_per_min,
            pipe_per_min,
            item_aliases,
            recipe_aliases,
            version: data.version.unwrap_or_default(),
            warnings: Vec::new(),
        })
    }

    fn invalid(&self, path: impl Into<String>, message: impl Into<String>) -> LoadError {
        LoadError::Invalid {
            file: self.file.clone(),
            path: path.into(),
            message: message.into(),
        }
    }

    /// The name in the names table, else the ID. A fallback is a warning.
    fn name(&mut self, kind: NameKind, id: &str) -> String {
        let table = match kind {
            NameKind::Item => &self.names.items,
            NameKind::Recipe => &self.names.recipes,
            NameKind::Region => &self.names.locations,
            NameKind::Category => &self.names.categories,
        };
        if let Some(name) = table.get(id) {
            return name.clone();
        }
        self.warnings.push(LoadWarning::MissingName {
            kind,
            id: id.to_owned(),
        });
        id.to_owned()
    }

    fn regions(&mut self, ids: &[String]) -> Result<Regions, LoadError> {
        let mut regions = IndexMap::new();
        for (i, id) in ids.iter().enumerate() {
            let region_id = RegionId::from(id.as_str());
            if regions.contains_key(&region_id) {
                return Err(
                    self.invalid(format!("regions[{i}]"), format!("duplicate region `{id}`"))
                );
            }
            let name = self.name(NameKind::Region, id);
            regions.insert(
                region_id.clone(),
                Region {
                    id: region_id,
                    name,
                },
            );
        }
        Ok(regions)
    }

    fn categories(&mut self, ids: &[String]) -> IndexMap<CategoryId, Category> {
        ids.iter()
            .map(|id| {
                let category_id = CategoryId::from(id.as_str());
                let name = self.name(NameKind::Category, id);
                (
                    category_id.clone(),
                    Category {
                        id: category_id,
                        name,
                    },
                )
            })
            .collect()
    }

    fn items(&mut self, entries: &[ItemEntry]) -> Result<Items, LoadError> {
        let mut items = IndexMap::new();
        for (i, entry) in entries.iter().enumerate() {
            let id = ItemId::from(entry.id.as_str());
            if items.contains_key(&id) {
                return Err(
                    self.invalid(format!("items[{i}].id"), format!("duplicate item `{id}`"))
                );
            }
            let item = Item {
                id: id.clone(),
                name: self.name(NameKind::Item, &entry.id),
                phase: entry.phase,
                color: entry.color.clone(),
            };
            items.insert(id, item);
        }
        Ok(items)
    }

    fn facilities(
        &mut self,
        entries: &[FacilityEntry],
        items: &Items,
        regions: &Regions,
    ) -> Result<Facilities, LoadError> {
        let mut facilities = IndexMap::new();
        for (i, entry) in entries.iter().enumerate() {
            let path = format!("facilities[{i}]");
            let id = FacilityId::from(entry.id.as_str());
            if facilities.contains_key(&id) {
                return Err(
                    self.invalid(format!("{path}.id"), format!("duplicate facility `{id}`"))
                );
            }
            check_positive(entry.speed).map_err(|m| self.invalid(format!("{path}.speed"), m))?;
            check_finite(entry.power_kw)
                .map_err(|m| self.invalid(format!("{path}.power_kw"), m))?;
            let activation = match &entry.activation {
                None => None,
                Some(activation) => {
                    let Some((item, _)) = items.get_key_value(activation.item.as_str()) else {
                        return Err(self.invalid(
                            format!("{path}.activation.item"),
                            format!("unknown item `{}`", activation.item),
                        ));
                    };
                    let per_min = check_positive(activation.per_min)
                        .map_err(|m| self.invalid(format!("{path}.activation.per_min"), m))?;
                    Some(ActivationInput::new(item.clone(), per_min))
                }
            };
            let facility = Facility {
                id: id.clone(),
                name: self.name(NameKind::Item, &entry.id),
                speed: entry.speed,
                power_kw: entry.power_kw,
                regions: self.region_ids(&entry.regions, regions, &format!("{path}.regions"))?,
                activation,
            };
            facilities.insert(id, facility);
        }
        Ok(facilities)
    }

    fn region_ids(
        &self,
        ids: &[String],
        regions: &Regions,
        path: &str,
    ) -> Result<Vec<RegionId>, LoadError> {
        ids.iter()
            .enumerate()
            .map(|(j, id)| match regions.get_key_value(id.as_str()) {
                Some((id, _)) => Ok(id.clone()),
                None => Err(self.invalid(format!("{path}[{j}]"), format!("unknown region `{id}`"))),
            })
            .collect()
    }

    fn recipes(
        &mut self,
        entries: &[RecipeEntry],
        items: &Items,
        facilities: &Facilities,
        regions: &Regions,
    ) -> Result<IndexMap<RecipeId, Recipe>, LoadError> {
        let mut recipes = IndexMap::new();
        for (i, entry) in entries.iter().enumerate() {
            let path = format!("recipes[{i}]");
            let id = RecipeId::from(entry.id.as_str());
            if recipes.contains_key(&id) {
                return Err(self.invalid(format!("{path}.id"), format!("duplicate recipe `{id}`")));
            }
            let time_s = check_positive(entry.time_s)
                .map_err(|m| self.invalid(format!("{path}.time_s"), m))?;
            let facility_ids = self.facility_ids(entry, facilities, &path)?;
            let mut inputs = self.quantities(&entry.inputs, items, &format!("{path}.in"))?;
            let outputs = self.quantities(&entry.outputs, items, &format!("{path}.out"))?;
            if let Some((item, qty)) = self.activation(entry, &facility_ids, facilities, &path)? {
                match inputs.iter_mut().find(|(input, _)| *input == item) {
                    Some((_, total)) => *total += qty,
                    None => inputs.push((item, qty)),
                }
            }
            let is_trade = entry.kind == Some(RecipeKind::Trade);
            if is_trade && (entry.inputs.len() != 1 || entry.outputs.is_empty()) {
                return Err(self.invalid(
                    format!("{path}.kind"),
                    "a trade takes exactly one item and gives at least one",
                ));
            }
            if let Some(power) = entry.power_kw {
                check_finite(power).map_err(|m| self.invalid(format!("{path}.power_kw"), m))?;
            }
            let environment = match &entry.environment {
                None => None,
                Some(gas) => match items.get_key_value(gas.as_str()) {
                    Some((gas, _)) => Some(gas.clone()),
                    None => {
                        return Err(self.invalid(
                            format!("{path}.environment"),
                            format!("unknown item `{gas}`"),
                        ));
                    }
                },
            };
            let recipe = Recipe {
                id: id.clone(),
                name: self.name(NameKind::Recipe, &entry.id),
                category: CategoryId::from(entry.category.as_str()),
                time_s,
                inputs,
                outputs,
                power_override_kw: entry.power_kw,
                is_mining: entry.kind == Some(RecipeKind::Mining),
                is_trade,
                hidden: entry.hidden || is_trade,
                facilities: facility_ids,
                regions: self.region_ids(&entry.regions, regions, &format!("{path}.regions"))?,
                environment,
            };
            recipes.insert(id, recipe);
        }
        Ok(recipes)
    }

    /// The activation input a recipe's facilities need, as a quantity per craft to add to its
    /// inputs: `per_min` over one craft at the facilities' speed. Facilities of one recipe that
    /// disagree on it can't share the recipe's inputs, so that is an error.
    fn activation(
        &self,
        entry: &RecipeEntry,
        facility_ids: &[FacilityId],
        facilities: &Facilities,
        path: &str,
    ) -> Result<Option<(ItemId, f64)>, LoadError> {
        let all: Vec<&Facility> = facility_ids.iter().map(|id| &facilities[id]).collect();
        let Some(first) = all.iter().find(|f| f.activation.is_some()) else {
            return Ok(None);
        };
        let activation = first
            .activation
            .as_ref()
            .expect("found one with an activation");
        if let Some(other) = all
            .iter()
            .find(|f| f.activation.as_ref() != Some(activation) || f.speed != first.speed)
        {
            return Err(self.invalid(
                format!("{path}.facilities"),
                format!(
                    "`{}` and `{}` differ in activation input or speed, so they can't share a \
                     recipe",
                    first.id, other.id
                ),
            ));
        }
        let qty = activation.per_min() * entry.time_s / (60.0 * first.speed);
        Ok(Some((activation.item().clone(), qty)))
    }

    fn facility_ids(
        &self,
        recipe: &RecipeEntry,
        facilities: &Facilities,
        path: &str,
    ) -> Result<Vec<FacilityId>, LoadError> {
        if recipe.facilities.is_empty() {
            return Err(self.invalid(
                format!("{path}.facilities"),
                "no facility can run this recipe",
            ));
        }
        recipe
            .facilities
            .iter()
            .enumerate()
            .map(|(j, id)| match facilities.get_key_value(id.as_str()) {
                Some((id, _)) => Ok(id.clone()),
                None => Err(self.invalid(
                    format!("{path}.facilities[{j}]"),
                    format!("unknown facility `{id}`"),
                )),
            })
            .collect()
    }

    fn quantities(
        &self,
        raw: &IndexMap<String, f64>,
        items: &Items,
        path: &str,
    ) -> Result<Vec<(ItemId, f64)>, LoadError> {
        raw.iter()
            .map(|(id, &qty)| {
                let Some((id, _)) = items.get_key_value(id.as_str()) else {
                    return Err(
                        self.invalid(format!("{path}.{id}"), format!("unknown item `{id}`"))
                    );
                };
                check_positive(qty).map_err(|m| self.invalid(format!("{path}.{id}"), m))?;
                Ok((id.clone(), qty))
            })
            .collect()
    }

    /// An alias goes from an ID this version doesn't have to one it does.
    #[allow(clippy::type_complexity)]
    fn aliases(
        &self,
        aliases: &Aliases,
        items: &Items,
        recipes: &IndexMap<RecipeId, Recipe>,
    ) -> Result<(IndexMap<String, ItemId>, IndexMap<String, RecipeId>), LoadError> {
        let item_aliases = self.alias_table(&aliases.items, items, "items", "item")?;
        let recipe_aliases = self.alias_table(&aliases.recipes, recipes, "recipes", "recipe")?;
        Ok((item_aliases, recipe_aliases))
    }

    fn alias_table<K, V>(
        &self,
        aliases: &IndexMap<String, String>,
        known: &IndexMap<K, V>,
        key: &str,
        what: &str,
    ) -> Result<IndexMap<String, K>, LoadError>
    where
        K: std::hash::Hash + Eq + Clone + std::borrow::Borrow<str>,
    {
        aliases
            .iter()
            .map(|(old, new)| {
                let path = format!("aliases.{key}.{old}");
                if known.contains_key(old.as_str()) {
                    return Err(self.invalid(
                        path,
                        format!("`{old}` is a {what} of this version, so it can't be an alias"),
                    ));
                }
                match known.get_key_value(new.as_str()) {
                    Some((new, _)) => Ok((old.clone(), new.clone())),
                    None => Err(self.invalid(path, format!("unknown {what} `{new}`"))),
                }
            })
            .collect()
    }
}

fn check_positive(value: f64) -> Result<f64, String> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(format!("expected a positive number, found {value}"))
    }
}

fn check_finite(value: f64) -> Result<f64, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("expected a number, found {value}"))
    }
}
