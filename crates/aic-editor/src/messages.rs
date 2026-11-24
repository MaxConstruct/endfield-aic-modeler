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
