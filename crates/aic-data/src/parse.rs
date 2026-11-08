//! Turns factoriolab bytes into a checked `Catalog`.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::de::DeserializeOwned;

use crate::catalog::{
    Catalog, Category, Facility, Icon, IconRect, Item, LoadWarning, NameKind, Recipe, Region,
};
use crate::error::LoadError;
use crate::ids::{CategoryId, FacilityId, ItemId, RecipeId, RegionId};
use crate::raw::{RawData, RawI18n, RawIcon, RawItem, RawRecipe, RawSpeed};

/// Side length of one cell in `icons.webp`.
const ICON_SIZE: u32 = 64;

/// The facility that moves items between regions. Its recipes are hidden from the recipe browser.
const DOMAIN_TRANSFER: &str = "__domain_transfer";

const FLUID_PREFIXES: [&str; 2] = ["liquid_", "gas_"];

type Items = IndexMap<ItemId, Item>;
type Facilities = IndexMap<FacilityId, Facility>;
type Regions = IndexMap<RegionId, Region>;

/// The file names that errors report.
pub(crate) struct Sources<'a> {
    pub data: &'a Path,
    pub i18n: &'a Path,
}

impl Catalog {
    /// Parses a catalog from the contents of `data.json`, a language file (`i18n/<lang>.json`) and,
    /// optionally, `icons.webp`. The icon bytes are kept as they are, not decoded.
    ///
    /// The version is what `data.json` declares; `aic_data::load` replaces it with the folder name.
    pub fn from_bytes(
        data_json: &[u8],
        i18n_json: &[u8],
        icons_webp: Option<&[u8]>,
    ) -> Result<Catalog, LoadError> {
        let sources = Sources {
            data: Path::new("data.json"),
            i18n: Path::new("i18n.json"),
        };
        parse(data_json, i18n_json, icons_webp, &sources)
    }
}

pub(crate) fn parse(
    data_json: &[u8],
    i18n_json: &[u8],
    icons_webp: Option<&[u8]>,
    sources: &Sources<'_>,
) -> Result<Catalog, LoadError> {
    let raw: RawData = from_json(data_json, sources.data)?;
    let i18n: RawI18n = from_json(i18n_json, sources.i18n)?;
    let mut builder = Builder {
        file: sources.data.to_path_buf(),
        i18n,
        warnings: Vec::new(),
    };
    let mut catalog = builder.build(raw)?;
    catalog.icons_webp = icons_webp.map(<[u8]>::to_vec);
    catalog.warnings = builder.warnings;
    Ok(catalog)
}

fn from_json<T: DeserializeOwned>(bytes: &[u8], file: &Path) -> Result<T, LoadError> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let json_error = |path: String, source| LoadError::Json {
        file: file.to_path_buf(),
        path,
        source,
    };
    let value = serde_path_to_error::deserialize(&mut de)
        .map_err(|e| json_error(e.path().to_string(), e.into_inner()))?;
    de.end().map_err(|e| json_error(".".to_owned(), e))?;
    Ok(value)
}

struct Builder {
    file: PathBuf,
    i18n: RawI18n,
    warnings: Vec<LoadWarning>,
}

impl Builder {
    fn build(&mut self, raw: RawData) -> Result<Catalog, LoadError> {
        let regions = self.regions(&raw)?;
        let categories = self.categories(&raw);
        let (items, facilities) = self.items(&raw.items, &regions)?;
        let recipes = self.recipes(&raw.recipes, &items, &facilities, &regions)?;
        let (belt_per_min, pipe_per_min) = self.capacities(&raw)?;
        Ok(Catalog {
            items,
            facilities,
            recipes,
            regions,
            categories,
            belt_per_min,
            pipe_per_min,
            icons: self.icons(&raw.icons)?,
            icons_webp: None,
            version: raw.version.values().next().cloned().unwrap_or_default(),
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

    /// The name in the language file, else the Chinese name, else the ID. A fallback is a warning.
    fn name(&mut self, kind: NameKind, id: &str, zh: Option<&str>) -> String {
        let table = match kind {
            NameKind::Item => &self.i18n.items,
            NameKind::Recipe => &self.i18n.recipes,
            NameKind::Region => &self.i18n.locations,
            NameKind::Category => &self.i18n.categories,
        };
        if let Some(name) = table.get(id) {
            return name.clone();
        }
        self.warnings.push(LoadWarning::MissingName {
            kind,
            id: id.to_owned(),
        });
        zh.unwrap_or(id).to_owned()
    }

    fn regions(&mut self, raw: &RawData) -> Result<Regions, LoadError> {
        let mut regions = IndexMap::new();
        for (i, region) in raw.locations.iter().enumerate() {
            let id = RegionId::from(region.id.as_str());
            if regions.contains_key(&id) {
                return Err(self.invalid(
                    format!("locations[{i}].id"),
                    format!("duplicate region `{id}`"),
                ));
            }
            let name = self.name(NameKind::Region, &region.id, region.name.as_deref());
            regions.insert(id.clone(), Region { id, name });
        }
        Ok(regions)
    }

    fn categories(&mut self, raw: &RawData) -> IndexMap<CategoryId, Category> {
        raw.categories
            .iter()
            .map(|category| {
                let id = CategoryId::from(category.id.as_str());
                let name = self.name(NameKind::Category, &category.id, category.name.as_deref());
                (id.clone(), Category { id, name })
            })
            .collect()
    }

    fn items(
        &mut self,
        raw: &[RawItem],
        regions: &Regions,
    ) -> Result<(Items, Facilities), LoadError> {
        let mut items = IndexMap::new();
        let mut facilities = IndexMap::new();
        for (i, item) in raw.iter().enumerate() {
            let id = ItemId::from(item.id.as_str());
            if items.contains_key(&id) {
                return Err(
                    self.invalid(format!("items[{i}].id"), format!("duplicate item `{id}`"))
                );
            }
            let name = self.name(NameKind::Item, &item.id, item.name.as_deref());
            if let Some(machine) = &item.machine {
                let path = format!("items[{i}].machine");
                check_positive(machine.speed)
                    .map_err(|m| self.invalid(format!("{path}.speed"), m))?;
                let facility = Facility {
                    id: FacilityId::from(item.id.as_str()),
                    name: name.clone(),
                    speed: machine.speed,
                    power_kw: machine.usage.unwrap_or(0.0),
                    regions: self.region_ids(
                        &machine.locations,
                        regions,
                        &format!("{path}.locations"),
                    )?,
                };
                facilities.insert(facility.id.clone(), facility);
            }
            let is_fluid = item.machine.is_none()
                && FLUID_PREFIXES
                    .iter()
                    .any(|prefix| item.id.starts_with(prefix));
            let item = Item {
                id: id.clone(),
                name,
                name_zh: item.name.clone().unwrap_or_else(|| item.id.clone()),
                category: CategoryId::from(item.category.as_str()),
                icon: item.icon.clone().unwrap_or_else(|| item.id.clone()),
                is_fluid,
            };
            items.insert(id, item);
        }
        Ok((items, facilities))
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
        raw: &[RawRecipe],
        items: &Items,
        facilities: &Facilities,
        regions: &Regions,
    ) -> Result<IndexMap<RecipeId, Recipe>, LoadError> {
        let mut recipes = IndexMap::new();
        for (i, recipe) in raw.iter().enumerate() {
            let path = format!("recipes[{i}]");
            let id = RecipeId::from(recipe.id.as_str());
            if recipes.contains_key(&id) {
                return Err(self.invalid(format!("{path}.id"), format!("duplicate recipe `{id}`")));
            }
            check_positive(recipe.time).map_err(|m| self.invalid(format!("{path}.time"), m))?;
            let facility_ids = self.facility_ids(recipe, facilities, &path)?;
            let recipe = Recipe {
                id: id.clone(),
                name: self.name(NameKind::Recipe, &recipe.id, recipe.name.as_deref()),
                category: CategoryId::from(recipe.category.as_str()),
                icon: recipe.icon.clone().unwrap_or_else(|| recipe.id.clone()),
                time_s: recipe.time,
                inputs: self.quantities(&recipe.inputs, items, &format!("{path}.in"))?,
                outputs: self.quantities(&recipe.outputs, items, &format!("{path}.out"))?,
                power_override_kw: recipe.usage,
                is_mining: has_flag(recipe, "mining"),
                hidden: has_flag(recipe, "hideProducer")
                    || facility_ids.iter().any(|f| f.as_str() == DOMAIN_TRANSFER),
                facilities: facility_ids,
                regions: self.region_ids(
                    &recipe.locations,
                    regions,
                    &format!("{path}.locations"),
                )?,
            };
            recipes.insert(id, recipe);
        }
        Ok(recipes)
    }

    fn facility_ids(
        &self,
        recipe: &RawRecipe,
        facilities: &Facilities,
        path: &str,
    ) -> Result<Vec<FacilityId>, LoadError> {
        if recipe.producers.is_empty() {
            return Err(self.invalid(
                format!("{path}.producers"),
                "no facility can run this recipe",
            ));
        }
        recipe
            .producers
            .iter()
            .enumerate()
            .map(|(j, id)| match facilities.get_key_value(id.as_str()) {
                Some((id, _)) => Ok(id.clone()),
                None => Err(self.invalid(
                    format!("{path}.producers[{j}]"),
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

    /// Belt and pipe capacity in items per minute.
    fn capacities(&self, raw: &RawData) -> Result<(f64, f64), LoadError> {
        let defaults = &raw.defaults;
        let belt = self.capacity(
            &raw.items,
            defaults.max_belt.as_deref(),
            "belt",
            "maxBelt",
            |item| item.belt.as_ref(),
        )?;
        let pipe = self.capacity(
            &raw.items,
            defaults.max_pipe.as_deref(),
            "pipe",
            "maxPipe",
            |item| item.pipe.as_ref(),
        )?;
        Ok((belt, pipe))
    }

    /// The capacity of the item that `defaults.<default_key>` names, or else of the first item that has
    /// `key`.
    fn capacity(
        &self,
        items: &[RawItem],
        preferred: Option<&str>,
        key: &str,
        default_key: &str,
        speed: fn(&RawItem) -> Option<&RawSpeed>,
    ) -> Result<f64, LoadError> {
        let found = items.iter().enumerate().find_map(|(i, item)| {
            let raw_speed = speed(item)?;
            preferred
                .is_none_or(|id| id == item.id)
                .then_some((i, raw_speed))
        });
        let Some((i, raw_speed)) = found else {
            return Err(match preferred {
                Some(id) => self.invalid(
                    format!("defaults.{default_key}"),
                    format!("`{id}` is not an item with `{key}`"),
                ),
                None => self.invalid("items", format!("no item has `{key}`")),
            });
        };
        check_positive(raw_speed.speed)
            .map_err(|m| self.invalid(format!("items[{i}].{key}.speed"), m))?;
        Ok(raw_speed.speed * 60.0)
    }

    fn icons(&self, raw: &[RawIcon]) -> Result<IndexMap<String, Icon>, LoadError> {
        raw.iter()
            .enumerate()
            .map(|(i, icon)| {
                let rect = parse_position(&icon.position).ok_or_else(|| {
                    self.invalid(
                        format!("icons[{i}].position"),
                        format!(
                            "expected two offsets like \"-64px 0px\", found \"{}\"",
                            icon.position
                        ),
                    )
                })?;
                let icon_out = Icon {
                    rect,
                    color: icon.color.clone(),
                };
                Ok((icon.id.clone(), icon_out))
            })
            .collect()
    }
}

fn has_flag(recipe: &RawRecipe, flag: &str) -> bool {
    recipe.flags.iter().any(|f| f == flag)
}

fn check_positive(value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("expected a positive number, found {value}"))
    }
}

/// `"-64px -128px"` is the cell at x = 64, y = 128. The offsets are negative because they move the
/// sprite sheet behind a fixed window.
fn parse_position(position: &str) -> Option<IconRect> {
    let offset = |part: &str| -> Option<u32> {
        let px: i64 = part.strip_suffix("px")?.parse().ok()?;
        u32::try_from(-px).ok()
    };
    let mut parts = position.split_whitespace();
    let (x, y) = (offset(parts.next()?)?, offset(parts.next()?)?);
    if parts.next().is_some() {
        return None;
    }
    Some(IconRect {
        x,
        y,
        w: ICON_SIZE,
        h: ICON_SIZE,
    })
}
