//! What the node picker offers and what a choice adds: no drawing. A UI shows the picker and
//! calls these to fill it.

use aic_core::{NodeId, NodeKind, Side};
use aic_data::{Catalog, FacilityId, ItemId, Recipe, RecipeId, RegionId};

use crate::names;

/// Where the picker was opened.
#[derive(Debug, Clone, PartialEq)]
pub enum Origin {
    /// Right-click on empty canvas: add a node on its own.
    Canvas,
    /// From a port of `item`: on an input, add a producer of it; on an output, a consumer. The new
    /// node is linked to the port.
    Port {
        node: NodeId,
        side: Side,
        item: ItemId,
    },
}

/// The node kinds that are not recipes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Source,
    Sink,
    Splitter,
    Merger,
    Storage,
    ControlPort,
}

const LOGISTICS: [Builtin; 4] = [
    Builtin::Splitter,
    Builtin::Merger,
    Builtin::Storage,
    Builtin::ControlPort,
];

#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Recipe(RecipeId),
    Builtin(Builtin),
    /// An empty workspace, offered from the canvas.
    Workspace,
}

/// Which parts of a recipe the search text is matched against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Matching {
    pub name: bool,
    pub inputs: bool,
    pub outputs: bool,
}

impl Default for Matching {
    fn default() -> Self {
        Self {
            name: true,
            inputs: false,
            outputs: true,
        }
    }
}

/// The built-in nodes offered for `origin`. A source or sink needs an item, so they are only
/// offered from a port.
pub fn builtins(origin: &Origin) -> Vec<Builtin> {
    let end = match origin {
        Origin::Canvas => None,
        Origin::Port {
            side: Side::Input, ..
        } => Some(Builtin::Source),
        Origin::Port {
            side: Side::Output, ..
        } => Some(Builtin::Sink),
    };
    end.into_iter().chain(LOGISTICS).collect()
}

/// The recipes offered for `origin`, in data order: those of every region, not hidden, that make
/// (from an input port) or take (from an output port) the port's item, and match `query`.
/// Settlement trades are offered as outpost orders, except from an input port: they make only
/// coupons.
pub fn recipes<'a>(
    catalog: &'a Catalog,
    origin: &Origin,
    query: &str,
    matching: Matching,
) -> Vec<&'a Recipe> {
    let query = query.trim().to_lowercase();
    let contains = |name: &str| name.to_lowercase().contains(&query);
    let any_item = |items: &[(ItemId, f64)]| {
        items
            .iter()
            .any(|(item, _)| contains(names::item(catalog, item.as_str())))
    };
    catalog
        .recipes()
        .values()
        .filter(|recipe| {
            let trade_ok = recipe.is_trade
                && !matches!(
                    origin,
                    Origin::Port {
                        side: Side::Input,
                        ..
                    }
                );
            !recipe.hidden || trade_ok
        })
        .filter(|recipe| match origin {
            Origin::Canvas => true,
            Origin::Port { side, item, .. } => {
                let items = match side {
                    Side::Input => &recipe.outputs,
                    Side::Output => &recipe.inputs,
                };
                items.iter().any(|(id, _)| id == item)
            }
        })
        .filter(|recipe| {
            query.is_empty()
                || (matching.name && contains(&recipe.name))
                || (matching.inputs && any_item(&recipe.inputs))
                || (matching.outputs && any_item(&recipe.outputs))
        })
        .collect()
}

/// The regions to mark a recipe with on a graph of `region`: the ones it is limited to, when
/// `region` is not among them. A recipe the graph can use needs no mark, so this is empty.
pub fn foreign_regions<'a>(region: &RegionId, recipe: &'a Recipe) -> &'a [RegionId] {
    if recipe.available_in(region) {
        &[]
    } else {
        &recipe.regions
    }
}

/// The node a built-in choice adds. A source and a control port need a limit; they start at one
/// belt (or pipe) of their item, for the player to change.
pub fn builtin_kind(catalog: &Catalog, builtin: Builtin, item: Option<&ItemId>) -> NodeKind {
    let one_lane = || {
        let fluid = item
            .and_then(|id| catalog.item(id.as_str()))
            .is_some_and(|item| item.is_fluid());
        if fluid {
            catalog.pipe_per_min()
        } else {
            catalog.belt_per_min()
        }
    };
    let item = item.cloned();
    match builtin {
        Builtin::Source => NodeKind::Source {
            item: item.expect("a source is offered only from a port"),
            limit: one_lane(),
        },
        Builtin::Sink => NodeKind::Sink {
            item: item.expect("a sink is offered only from a port"),
            limit: None,
        },
        Builtin::Splitter => NodeKind::Splitter { item },
        Builtin::Merger => NodeKind::Merger { item },
        Builtin::Storage => NodeKind::Storage { item },
        Builtin::ControlPort => NodeKind::ControlPort {
            item,
            limit: one_lane(),
        },
    }
}

/// A new recipe node for a graph of `region`: on the first of the recipe's facilities that the
/// region has (else its first), with no limit, so the solver sizes it from the rest of the graph.
/// On a facility of whole units (a Gas Dispersing Unit, a Forge of Luminance) it runs one: how
/// many a plan needs is the player's to say (ADR 0015). A settlement trade becomes an outpost
/// order.
pub fn recipe_node(catalog: &Catalog, region: &RegionId, recipe: RecipeId) -> Option<NodeKind> {
    if let Some(order) = NodeKind::outpost_order(catalog, &recipe) {
        return Some(order);
    }
    let facilities = &catalog.recipe(recipe.as_str())?.facilities;
    let buildable = |id: &&FacilityId| {
        (catalog.facility(id.as_str())).is_some_and(|facility| facility.available_in(region))
    };
    let facility = (facilities.iter().find(buildable))
        .or(facilities.first())?
        .clone();
    let whole_units = (catalog.facility(facility.as_str())).is_some_and(|f| f.whole_units());
    Some(NodeKind::Recipe {
        recipe,
        facility,
        limit: whole_units.then_some(1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        crate::testing::fixture_catalog()
    }

    fn ids(recipes: Vec<&Recipe>) -> Vec<&str> {
        recipes.into_iter().map(|r| r.id.as_str()).collect()
    }

    fn port(side: Side, item: &str) -> Origin {
        Origin::Port {
            node: NodeId::default(),
            side,
            item: item.into(),
        }
    }

    #[test]
    fn canvas_lists_every_regions_visible_recipes_in_data_order() {
        let catalog = catalog();
        // Every region's recipes: one only Wuling has is offered too.
        assert_eq!(
            ids(recipes(&catalog, &Origin::Canvas, "", Matching::default())),
            [
                "iron_ore",
                "iron_ingot",
                "iron_plate",
                "iron_rod",
                "screw",
                "reinforced_plate",
                "modular_frame",
                "iron_wire",
                "coupon-iron_plate"
            ]
        );
    }

    #[test]
    fn an_input_port_lists_producers_including_mining() {
        let catalog = catalog();
        let origin = port(Side::Input, "iron_ore");
        assert_eq!(
            ids(recipes(&catalog, &origin, "", Matching::default())),
            ["iron_ore"]
        );
        assert_eq!(builtins(&origin)[0], Builtin::Source);
    }

    #[test]
    fn an_output_port_lists_consumers() {
        let catalog = catalog();
        let origin = port(Side::Output, "iron_ingot");
        assert_eq!(
            ids(recipes(&catalog, &origin, "", Matching::default())),
            ["iron_plate", "iron_rod", "iron_wire"]
        );
        assert_eq!(builtins(&origin)[0], Builtin::Sink);
        assert!(!builtins(&Origin::Canvas).contains(&Builtin::Source));
    }

    #[test]
    fn search_matches_the_chosen_parts_case_insensitively() {
        let catalog = catalog();
        let by_name = Matching {
            name: true,
            inputs: false,
            outputs: false,
        };
        assert_eq!(
            ids(recipes(&catalog, &Origin::Canvas, "PLATE", by_name)),
            ["iron_plate", "reinforced_plate", "coupon-iron_plate"]
        );
        let by_inputs = Matching {
            name: false,
            inputs: true,
            outputs: false,
        };
        assert_eq!(
            ids(recipes(&catalog, &Origin::Canvas, "iron rod", by_inputs)),
            ["screw", "modular_frame"]
        );
    }

    #[test]
    fn a_source_starts_at_one_belt() {
        let catalog = catalog();
        let kind = builtin_kind(&catalog, Builtin::Source, Some(&"iron_ore".into()));
        assert_eq!(
            kind,
            NodeKind::Source {
                item: "iron_ore".into(),
                limit: 30.0
            }
        );
    }

    /// Ore on a rig only Valley IV has, listed first, and on one built anywhere.
    fn two_rigs() -> Catalog {
        let data = br#"{
            "format": 1, "belt_per_min": 30, "pipe_per_min": 120,
            "regions": ["tundra", "jinlong"],
            "items": [{ "id": "ore" }],
            "facilities": [
                { "id": "valley_rig", "regions": ["tundra"] },
                { "id": "rig" },
                { "id": "wuling_rig", "regions": ["jinlong"] }
            ],
            "recipes": [
                { "id": "ore", "category": "material", "time_s": 3, "out": { "ore": 1 },
                  "facilities": ["valley_rig", "rig"], "kind": "mining" },
                { "id": "deep_ore", "category": "material", "time_s": 3, "out": { "ore": 2 },
                  "facilities": ["wuling_rig"], "kind": "mining" }
            ]
        }"#;
        let names = aic_data::Names {
            file: "names.json",
            json: b"{}",
        };
        Catalog::from_bytes(data, names).expect("test catalog loads")
    }

    fn facility_of(kind: Option<NodeKind>) -> String {
        match kind {
            Some(NodeKind::Recipe { facility, .. }) => facility.to_string(),
            other => panic!("expected a recipe node, got {other:?}"),
        }
    }

    #[test]
    fn only_a_recipe_the_graphs_region_lacks_is_marked_with_its_regions() {
        let catalog = catalog();
        let marks = |region: &str, id: &str| {
            foreign_regions(&region.into(), catalog.recipe(id).unwrap()).to_vec()
        };
        assert_eq!(marks("tundra", "iron_wire"), ["jinlong".into()]);
        assert!(marks("jinlong", "iron_wire").is_empty());
        assert!(marks("tundra", "iron_plate").is_empty());
    }

    #[test]
    fn a_new_recipe_node_runs_on_the_first_facility_its_region_has() {
        let catalog = two_rigs();
        let node = |region: &str, recipe: &str| {
            facility_of(recipe_node(&catalog, &region.into(), recipe.into()))
        };
        assert_eq!(node("tundra", "ore"), "valley_rig");
        // Wuling has no valley rig, so the node starts on the one it can build.
        assert_eq!(node("jinlong", "ore"), "rig");
        // A recipe the region can't run at all still lands on its first facility, with the
        // solver's region warning.
        assert_eq!(node("tundra", "deep_ore"), "wuling_rig");
    }

    #[test]
    fn a_new_node_of_whole_units_runs_one_and_any_other_has_no_limit() {
        let data = br#"{
            "format": 1, "belt_per_min": 30, "pipe_per_min": 120,
            "regions": ["jinlong"],
            "items": [{ "id": "gas_inert", "phase": "gas" }, { "id": "ore" }],
            "facilities": [{ "id": "vaporizer_1", "whole_units": true }, { "id": "rig" }],
            "recipes": [
                { "id": "vaporizer_1-gas_inert", "category": "material", "time_s": 60,
                  "in": { "gas_inert": 6 }, "facilities": ["vaporizer_1"],
                  "creates_environment": "gas_inert" },
                { "id": "ore", "category": "material", "time_s": 3, "out": { "ore": 1 },
                  "facilities": ["rig"], "kind": "mining" }
            ]
        }"#;
        let names = aic_data::Names {
            file: "names.json",
            json: b"{}",
        };
        let catalog = Catalog::from_bytes(data, names).expect("test catalog loads");
        let limit = |recipe: &str| match recipe_node(&catalog, &"jinlong".into(), recipe.into()) {
            Some(NodeKind::Recipe { limit, .. }) => limit,
            other => panic!("expected a recipe node, got {other:?}"),
        };
        // How many units a plan needs is the player's to say (ADR 0015).
        assert_eq!(limit("vaporizer_1-gas_inert"), Some(1.0));
        assert_eq!(limit("ore"), None);
    }
}
