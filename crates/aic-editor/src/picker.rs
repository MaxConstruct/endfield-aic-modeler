//! What the node picker offers and what a choice adds: no drawing. A UI shows the picker and
//! calls these to fill it.

use aic_core::{NodeId, NodeKind, Side};
use aic_data::{Catalog, ItemId, Recipe, RecipeId};

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

/// The node a built-in choice adds. A source and a control port need a limit; they start at one
/// belt (or pipe) of their item, for the player to change.
pub fn builtin_kind(catalog: &Catalog, builtin: Builtin, item: Option<&ItemId>) -> NodeKind {
    let one_lane = || {
        let fluid = item
            .and_then(|id| catalog.item(id.as_str()))
            .is_some_and(|item| item.is_fluid);
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

/// A new recipe node: on the recipe's first facility, with no limit, so the solver sizes it from the
/// rest of the graph. A settlement trade becomes an outpost order.
pub fn recipe_node(catalog: &Catalog, recipe: RecipeId) -> Option<NodeKind> {
    if let Some(order) = NodeKind::outpost_order(catalog, &recipe) {
        return Some(order);
    }
    let facility = catalog.recipe(recipe.as_str())?.facilities.first()?.clone();
    Some(NodeKind::Recipe {
        recipe,
        facility,
        limit: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &[u8] = include_bytes!("../fixtures/satisfactory/vtest/data.json");
    const EN: &[u8] = include_bytes!("../fixtures/satisfactory/vtest/i18n/en.json");

    fn catalog() -> Catalog {
        Catalog::from_bytes(DATA, EN, None).expect("fixture loads")
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
        // Every region's recipes: one only Wuling has is offered too, with its badge.
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
}
