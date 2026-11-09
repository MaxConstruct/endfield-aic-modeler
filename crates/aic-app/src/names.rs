//! Display names. IDs are game IDs; names come from the catalog's language table at render time,
//! falling back to the ID when the data has no name.

use aic_core::NodeKind;
use aic_data::{Catalog, ItemId};

use crate::i18n::Strings;

pub fn item<'a>(catalog: &'a Catalog, id: &'a str) -> &'a str {
    catalog.item(id).map_or(id, |item| item.name.as_str())
}

/// The item of a port, or "?" for a logistics node that has none yet.
pub fn maybe_item(catalog: &Catalog, strings: &Strings, id: Option<&ItemId>) -> String {
    match id {
        Some(id) => item(catalog, id.as_str()).to_owned(),
        None => strings.get("item-unset"),
    }
}

pub fn recipe<'a>(catalog: &'a Catalog, id: &'a str) -> &'a str {
    catalog.recipe(id).map_or(id, |recipe| recipe.name.as_str())
}

pub fn facility<'a>(catalog: &'a Catalog, id: &'a str) -> &'a str {
    catalog
        .facility(id)
        .map_or(id, |facility| facility.name.as_str())
}

pub fn region<'a>(catalog: &'a Catalog, id: &'a str) -> &'a str {
    catalog.region(id).map_or(id, |region| region.name.as_str())
}

/// The Fluent key naming a node's kind.
pub fn kind_key(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Recipe { .. } => "kind-recipe",
        NodeKind::Source { .. } => "kind-source",
        NodeKind::Sink { .. } => "kind-sink",
        NodeKind::OutpostOrder { .. } => "kind-outpost-order",
        NodeKind::Splitter { .. } => "kind-splitter",
        NodeKind::Merger { .. } => "kind-merger",
        NodeKind::Storage { .. } => "kind-storage",
        NodeKind::ControlPort { .. } => "kind-control-port",
        NodeKind::MissingRecipe { .. } => "kind-missing-recipe",
    }
}

/// What a node is about: its recipe, or the item it handles.
pub fn node_subject(catalog: &Catalog, strings: &Strings, kind: &NodeKind) -> String {
    match kind {
        NodeKind::Recipe { recipe: id, .. } => recipe(catalog, id.as_str()).to_owned(),
        NodeKind::MissingRecipe { recipe, .. } => recipe.to_string(),
        NodeKind::OutpostOrder { recipe: id, .. } => recipe(catalog, id.as_str()).to_owned(),
        _ => maybe_item(catalog, strings, kind.item()),
    }
}

/// A one-line name for a node, for the inspector and throttle reasons.
pub fn node_title(catalog: &Catalog, strings: &Strings, kind: &NodeKind) -> String {
    match kind {
        NodeKind::Recipe { .. } => node_subject(catalog, strings, kind),
        _ => strings.format(
            "node-title",
            &[
                ("kind", &strings.get(kind_key(kind))),
                ("subject", &node_subject(catalog, strings, kind)),
            ],
        ),
    }
}

/// "1 node", "3 nodes".
pub fn node_count(strings: &Strings, count: usize) -> String {
    if count == 1 {
        strings.get("group-node-count-one")
    } else {
        strings.format("group-node-count", &[("count", &count.to_string())])
    }
}
