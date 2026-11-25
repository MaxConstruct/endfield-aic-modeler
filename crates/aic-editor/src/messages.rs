//! Words for what the solver and the graph report: refusals, warnings and rates. Pure `Strings`
//! formatting of core types, kept apart from the drawing.

use aic_core::{Graph, Issue, LinkError, NodeKind, Side, UndeterminedReason, Warning};
use aic_data::{Catalog, ItemId};

use crate::format;
use crate::i18n::Strings;
use crate::names;

/// Why a link was refused, in the player's words.
pub fn link_error(catalog: &Catalog, strings: &Strings, error: &LinkError) -> String {
    let item = |id: &aic_data::ItemId| names::item(catalog, id.as_str()).to_owned();
    match error {
        LinkError::ItemMismatch { from, to } => strings.format(
            "link-error-mismatch",
            &[("from", &item(from)), ("to", &item(to))],
        ),
        LinkError::SolidFluid { from, to } => strings.format(
            "link-error-solid-fluid",
            &[("from", &item(from)), ("to", &item(to))],
        ),
        LinkError::NoItem => strings.get("link-error-no-item"),
        LinkError::SelfLink => strings.get("link-error-self"),
        LinkError::Duplicate => strings.get("link-error-duplicate"),
        LinkError::AcrossWorkspaces => strings.get("link-error-across-workspaces"),
        LinkError::UnknownNode(_) | LinkError::NoSuchPort { .. } => strings.get("link-error-other"),
    }
}

/// A rate as `12 /min`, or nothing when it is not known.
pub fn per_min(strings: &Strings, value: Option<f64>) -> String {
    match value {
        Some(_) => strings.format("totals-per-min", &[("rate", &format::short(value))]),
        None => String::new(),
    }
}

/// A warning in the player's words.
pub fn warning_text(
    catalog: &Catalog,
    strings: &Strings,
    graph: &Graph,
    warning: &Warning,
) -> String {
    let item = |id: &ItemId| names::item(catalog, id.as_str()).to_owned();
    match warning {
        Warning::Undetermined { reason, .. } => strings.get(match reason {
            UndeterminedReason::NoLimit => "warning-undetermined-no-limit",
            UndeterminedReason::Unbounded => "warning-undetermined-unbounded",
        }),
        Warning::PortUnlinked { port, outside } => {
            let node = graph.node(*port);
            let workspace = node
                .and_then(|n| n.workspace)
                .and_then(|w| graph.workspace_info(w))
                .map(|w| w.name.clone())
                .unwrap_or_default();
            let item = names::maybe_item(catalog, strings, node.and_then(|n| n.kind.item()));
            let input = matches!(
                node.map(|n| &n.kind),
                Some(NodeKind::WorkspacePort {
                    side: Side::Input,
                    ..
                })
            );
            let key = match (input, outside) {
                (true, true) => "warning-port-in-unlinked-outside",
                (true, false) => "warning-port-in-unlinked-inside",
                (false, true) => "warning-port-out-unlinked-outside",
                (false, false) => "warning-port-out-unlinked-inside",
            };
            strings.format(key, &[("workspace", &workspace), ("item", &item)])
        }
        Warning::Failed { message, .. } => {
            strings.format("warning-failed", &[("message", message)])
        }
        Warning::BeltOverCapacity { link, lanes } => {
            let link_item = graph.link(*link).map(|l| l.item.clone());
            let fluid = link_item
                .as_ref()
                .and_then(|id| catalog.item(id.as_str()))
                .is_some_and(|i| i.is_fluid());
            let name = link_item.as_ref().map(item).unwrap_or_default();
            let key = if fluid {
                "warning-pipe-over"
            } else {
                "warning-belt-over"
            };
            strings.format(key, &[("item", &name), ("lanes", &lanes.to_string())])
        }
        Warning::ResourceOverUse {
            item: id,
            used,
            available,
        } => strings.format(
            "warning-resource-over",
            &[
                ("item", &item(id)),
                ("used", &format::short(Some(*used))),
                ("available", &format::short(Some(*available))),
            ],
        ),
        Warning::EnvironmentMissing { node, gas } => {
            let recipe = (graph.node(*node))
                .map(|node| names::node_subject(catalog, strings, &node.kind))
                .unwrap_or_default();
            let environment = names::environment(catalog, strings, gas);
            // The facility whose recipe spreads the gas: the Gas Dispersing Unit.
            let unit = (catalog.recipes().values())
                .find(|recipe| recipe.creates_environment.as_ref() == Some(gas))
                .and_then(|recipe| recipe.facilities.first());
            let args = [("recipe", recipe.as_str()), ("environment", &environment)];
            match unit {
                Some(unit) => strings.format(
                    "warning-environment-missing",
                    &[
                        args[0],
                        args[1],
                        ("unit", names::facility(catalog, unit.as_str())),
                        ("gas", names::item(catalog, gas.as_str())),
                    ],
                ),
                None => strings.format("warning-environment-missing-no-unit", &args),
            }
        }
        Warning::ProjectionsShort {
            projection,
            needed,
            provided,
        } => {
            let (needed, provided) = (needed.to_string(), provided.to_string());
            let args = [
                ("array", names::facility(catalog, projection.as_str())),
                ("needed", needed.as_str()),
                ("provided", provided.as_str()),
            ];
            // The first facility whose recipe projects it: a Forge of Luminance.
            let forge = (catalog.recipes().values())
                .find(|recipe| recipe.projects.iter().any(|(p, _)| p == projection))
                .and_then(|recipe| recipe.facilities.first());
            match forge {
                Some(forge) => {
                    let forge = ("forge", names::facility(catalog, forge.as_str()));
                    strings.format(
                        "warning-projections-short",
                        &[args[0], args[1], args[2], forge],
                    )
                }
                None => strings.format("warning-projections-short-no-forge", &args),
            }
        }
        Warning::Issue(issue) => issue_text(catalog, strings, issue),
    }
}

pub fn issue_text(catalog: &Catalog, strings: &Strings, issue: &Issue) -> String {
    match issue {
        Issue::BadLink { error, .. } => strings.format(
            "warning-issue-bad-link",
            &[("error", &link_error(catalog, strings, error))],
        ),
        Issue::MissingRecipe { recipe, .. } => strings.format(
            "warning-issue-missing-recipe",
            &[("recipe", recipe.as_str())],
        ),
        Issue::UnknownItem { item, .. } => {
            strings.format("warning-issue-unknown-item", &[("item", item.as_str())])
        }
        Issue::RecipeOutsideRegion { recipe, .. } => strings.format(
            "warning-issue-recipe-region",
            &[("recipe", names::recipe(catalog, recipe.as_str()))],
        ),
        Issue::FacilityOutsideRegion { facility, .. } => strings.format(
            "warning-issue-facility-region",
            &[("facility", names::facility(catalog, facility.as_str()))],
        ),
        Issue::FacilityCannotRun { facility, .. } => strings.format(
            "warning-issue-facility-cannot-run",
            &[("facility", names::facility(catalog, facility.as_str()))],
        ),
        Issue::InvalidLimit { .. } => strings.get("warning-issue-invalid-limit"),
    }
}

#[cfg(test)]
mod tests {
    use aic_core::{NodeId, NodeKind, Pos};

    use super::*;

    fn recipe_node(graph: &mut Graph, recipe: &str, facility: &str) -> NodeId {
        graph.add_node(
            NodeKind::Recipe {
                recipe: recipe.into(),
                facility: facility.into(),
                limit: None,
            },
            Pos::default(),
        )
    }

    #[test]
    fn a_missing_environment_names_it_and_what_to_add() {
        let catalog = crate::testing::real_catalog().expect("the game's data is built in");
        let strings = Strings::english();
        let mut graph = Graph::new("jinlong".into());
        let node = recipe_node(
            &mut graph,
            "xiranite_oven_xiranite_powder_2",
            "xiranite_oven_1",
        );
        let warning = Warning::EnvironmentMissing {
            node,
            gas: "gas_inert".into(),
        };
        assert_eq!(
            warning_text(&catalog, &strings, &graph, &warning),
            "Xiranite Production (Carbon) needs the Stable Environment, and nothing in the graph \
             creates it. Add a Gas Dispersing Unit with Inergen."
        );
    }

    #[test]
    fn short_projections_name_the_array_and_both_counts() {
        let catalog = crate::testing::real_catalog().expect("the game's data is built in");
        let strings = Strings::english();
        let graph = Graph::new("jinlong".into());
        let warning = Warning::ProjectionsShort {
            projection: "magic_ming_1".into(),
            needed: 2,
            provided: 1,
        };
        assert_eq!(
            warning_text(&catalog, &strings, &graph, &warning),
            "The arrays need 2 L-Array T1 projections, but the forges project 1. Add a Forge of \
             Luminance."
        );
    }
}
