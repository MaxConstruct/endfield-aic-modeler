//! Builds small catalogs in code, so each test states exactly the recipes it uses.

#![allow(
    dead_code,
    reason = "each test file uses a different part of this module"
)]

use aic_core::{Graph, NodeId, NodeKind, PortRef, Pos};
use aic_data::{Catalog, ItemId, RegionId};
use serde_json::{Value, json};

pub const TUNDRA: &str = "tundra";
pub const JINLONG: &str = "jinlong";

/// A catalog written in the data file format and loaded through `Catalog::from_bytes`, like real
/// data.
#[derive(Default)]
pub struct TestCatalog {
    items: Vec<String>,
    facilities: Vec<Value>,
    recipes: Vec<Value>,
    item_aliases: serde_json::Map<String, Value>,
    recipe_aliases: serde_json::Map<String, Value>,
}

impl TestCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// A facility with speed 1 and the given power draw, available in every region.
    pub fn facility(self, id: &str, power_kw: f64) -> Self {
        self.facility_with(id, 1.0, power_kw, &[])
    }

    pub fn facility_with(mut self, id: &str, speed: f64, power_kw: f64, regions: &[&str]) -> Self {
        self.facilities.push(json!({
            "id": id, "speed": speed, "power_kw": power_kw, "regions": regions,
        }));
        self
    }

    /// A recipe available in every region.
    pub fn recipe(
        self,
        id: &str,
        facility: &str,
        time_s: f64,
        inputs: &[(&str, f64)],
        outputs: &[(&str, f64)],
    ) -> Self {
        self.recipe_with(id, &[facility], time_s, inputs, outputs, None, &[])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn recipe_with(
        mut self,
        id: &str,
        facilities: &[&str],
        time_s: f64,
        inputs: &[(&str, f64)],
        outputs: &[(&str, f64)],
        power_kw: Option<f64>,
        regions: &[&str],
    ) -> Self {
        for (item, _) in inputs.iter().chain(outputs) {
            self.know(item);
        }
        let map = |pairs: &[(&str, f64)]| -> Value {
            pairs
                .iter()
                .map(|(item, qty)| ((*item).to_owned(), json!(qty)))
                .collect()
        };
        let mut recipe = json!({
            "id": id, "category": "material", "time_s": time_s,
            "in": map(inputs), "out": map(outputs), "facilities": facilities, "regions": regions,
        });
        if let Some(power) = power_kw {
            recipe["power_kw"] = json!(power);
        }
        self.recipes.push(recipe);
        self
    }

    fn know(&mut self, item: &str) {
        if !self.items.iter().any(|known| known == item) {
            self.items.push(item.to_owned());
        }
    }

    /// Gives `facility`, added before, an activation input, as real data does. It comes on top of
    /// the inputs its recipes are given: each recipe then takes `per_min` more of `item` per
    /// facility flat out.
    pub fn activation(mut self, facility: &str, item: &str, per_min: f64) -> Self {
        self.know(item);
        let entry = (self.facilities.iter_mut())
            .find(|entry| entry["id"] == facility)
            .expect("the facility is added first");
        entry["activation"] = json!({ "item": item, "per_min": per_min });
        self
    }

    /// A mining recipe: no inputs, of kind `mining`, available in every region.
    pub fn mining(mut self, id: &str, facility: &str, time_s: f64, output: (&str, f64)) -> Self {
        self = self.recipe(id, facility, time_s, &[], &[output]);
        let recipe = self.recipes.last_mut().expect("just added");
        recipe["kind"] = json!("mining");
        self
    }

    /// A settlement trade: `coupons` of `coupon` per item of `input`.
    pub fn outpost_order(
        mut self,
        id: &str,
        facility: &str,
        input: &str,
        coupon: &str,
        coupons: f64,
    ) -> Self {
        self = self.recipe(id, facility, 2.0, &[(input, 1.0)], &[(coupon, coupons)]);
        let recipe = self.recipes.last_mut().expect("just added");
        recipe["kind"] = json!("trade");
        self
    }

    /// Says an older data version called item `new` `old`.
    pub fn item_alias(mut self, old: &str, new: &str) -> Self {
        self.item_aliases.insert(old.to_owned(), json!(new));
        self
    }

    /// Says an older data version called recipe `new` `old`.
    pub fn recipe_alias(mut self, old: &str, new: &str) -> Self {
        self.recipe_aliases.insert(old.to_owned(), json!(new));
        self
    }

    pub fn build(self) -> Catalog {
        let data = json!({
            "format": 1,
            "version": "test",
            "belt_per_min": 30,
            "pipe_per_min": 120,
            "regions": [TUNDRA, JINLONG],
            "items": self.items.iter().map(|id| item_entry(id)).collect::<Vec<_>>(),
            "facilities": self.facilities,
            "recipes": self.recipes,
            "aliases": { "items": self.item_aliases, "recipes": self.recipe_aliases },
        });
        let names = aic_data::Names {
            file: "names.json",
            json: b"{}",
        };
        Catalog::from_bytes(data.to_string().as_bytes(), names).expect("test catalog loads")
    }
}

/// An item whose ID starts with `liquid_` is a liquid and one with `gas_` a gas, as the game's IDs
/// go; any other is solid.
fn item_entry(id: &str) -> Value {
    let phase = if id.starts_with("liquid_") {
        "liquid"
    } else if id.starts_with("gas_") {
        "gas"
    } else {
        "solid"
    };
    json!({ "id": id, "phase": phase })
}

pub fn region(id: &str) -> RegionId {
    RegionId::from(id)
}

pub fn item(id: &str) -> ItemId {
    ItemId::from(id)
}

/// Adds a recipe node on the recipe's first facility.
pub fn add_recipe(
    graph: &mut Graph,
    catalog: &Catalog,
    recipe: &str,
    limit: Option<f64>,
) -> NodeId {
    let facility = catalog.recipe(recipe).expect("recipe exists").facilities[0].clone();
    graph.add_node(
        NodeKind::Recipe {
            recipe: recipe.into(),
            facility,
            limit,
        },
        Pos::default(),
    )
}

pub fn add(graph: &mut Graph, kind: NodeKind) -> NodeId {
    graph.add_node(kind, Pos::default())
}

/// Links `item` from `from`'s output to `to`'s input. Logistics nodes ignore the item on their side.
pub fn link(
    graph: &mut Graph,
    catalog: &Catalog,
    from: NodeId,
    to: NodeId,
    item_id: &str,
) -> aic_core::LinkId {
    graph
        .connect(
            catalog,
            PortRef::new(from, item_id),
            PortRef::new(to, item_id),
        )
        .expect("link is valid")
}

/// Copies these nodes, at the top of the graph.
pub fn copy_nodes(graph: &Graph, nodes: &[NodeId]) -> Graph {
    let members: Vec<aic_core::Member> = nodes.iter().map(|&n| aic_core::Member::Node(n)).collect();
    graph.copy_members(&members)
}

/// Pastes `copy` at the top of the graph, returning the new nodes there.
pub fn paste_nodes(
    graph: &mut Graph,
    catalog: &aic_data::Catalog,
    copy: &Graph,
    offset: aic_core::Pos,
) -> Vec<NodeId> {
    graph
        .paste_into(catalog, copy, offset, None)
        .into_iter()
        .filter_map(|member| match member {
            aic_core::Member::Node(node) => Some(node),
            aic_core::Member::Workspace(_) => None,
        })
        .collect()
}
