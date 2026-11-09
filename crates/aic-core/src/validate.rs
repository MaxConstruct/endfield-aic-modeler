//! Checks a graph against the catalog. A graph built through `Graph::connect` passes the link checks
//! by construction; a hand-edited or outdated save file may not.

use std::collections::HashSet;

use aic_data::{Catalog, FacilityId, ItemId, RecipeId};

use crate::graph::{Graph, LinkError, LinkId, NodeId, NodeKind, PortRef};

/// A problem with a graph. The solver leaves out the nodes and links involved.
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    /// The link breaks a linking rule.
    BadLink {
        link: LinkId,
        error: LinkError,
    },
    /// The recipe or facility no longer exists in this data version.
    MissingRecipe {
        node: NodeId,
        recipe: RecipeId,
    },
    UnknownItem {
        node: NodeId,
        item: ItemId,
    },
    RecipeOutsideRegion {
        node: NodeId,
        recipe: RecipeId,
    },
    FacilityOutsideRegion {
        node: NodeId,
        facility: FacilityId,
    },
    /// The recipe doesn't list this facility.
    FacilityCannotRun {
        node: NodeId,
        facility: FacilityId,
    },
    /// A limit that is negative or not a number.
    InvalidLimit {
        node: NodeId,
    },
}

impl Issue {
    /// Whether the solver can still use the node or link this issue is about. Region issues are
    /// warnings: the numbers still make sense.
    pub(crate) fn excludes(&self) -> bool {
        !matches!(
            self,
            Self::RecipeOutsideRegion { .. }
                | Self::FacilityOutsideRegion { .. }
                | Self::FacilityCannotRun { .. }
        )
    }
}

impl Graph {
    /// Every problem with this graph, nodes first, in graph order.
    pub fn validate(&self, catalog: &Catalog) -> Vec<Issue> {
        let mut issues = Vec::new();
        for (id, node) in &self.nodes {
            self.validate_node(catalog, id, &node.kind, &mut issues);
        }
        let mut seen = HashSet::new();
        for (id, link) in &self.links {
            let touches_missing = [link.from, link.to].iter().any(|node| {
                matches!(
                    self.nodes.get(*node).map(|n| &n.kind),
                    Some(NodeKind::MissingRecipe { .. })
                )
            });
            if touches_missing {
                continue;
            }
            let from = PortRef::new(link.from, link.item.clone());
            let to = PortRef::new(link.to, link.item.clone());
            let error = match self.check_link(catalog, &from, &to) {
                Ok(item) if item != link.item => Some(LinkError::ItemMismatch {
                    from: item,
                    to: link.item.clone(),
                }),
                Ok(_) if !seen.insert((link.from, link.to, link.item.clone())) => {
                    Some(LinkError::Duplicate)
                }
                Ok(_) => None,
                Err(error) => Some(error),
            };
            if let Some(error) = error {
                issues.push(Issue::BadLink { link: id, error });
            }
        }
        issues
    }

    fn validate_node(
        &self,
        catalog: &Catalog,
        id: NodeId,
        kind: &NodeKind,
        issues: &mut Vec<Issue>,
    ) {
        let limit_ok = |limit: Option<f64>| limit.is_none_or(|v| v.is_finite() && v >= 0.0);
        let limit = match kind {
            NodeKind::Recipe { limit, .. }
            | NodeKind::Sink { limit, .. }
            | NodeKind::OutpostOrder { limit, .. } => *limit,
            NodeKind::Source { limit, .. } | NodeKind::ControlPort { limit, .. } => Some(*limit),
            _ => None,
        };
        if !limit_ok(limit) {
            issues.push(Issue::InvalidLimit { node: id });
        }
        match kind {
            NodeKind::Recipe {
                recipe, facility, ..
            } => {
                let (Some(found), Some(found_facility)) = (
                    catalog.recipe(recipe.as_str()),
                    catalog.facility(facility.as_str()),
                ) else {
                    issues.push(Issue::MissingRecipe {
                        node: id,
                        recipe: recipe.clone(),
                    });
                    return;
                };
                if !found.available_in(&self.region) {
                    issues.push(Issue::RecipeOutsideRegion {
                        node: id,
                        recipe: recipe.clone(),
                    });
                }
                if !found_facility.available_in(&self.region) {
                    issues.push(Issue::FacilityOutsideRegion {
                        node: id,
                        facility: facility.clone(),
                    });
                }
                if !found.facilities.contains(facility) {
                    issues.push(Issue::FacilityCannotRun {
                        node: id,
                        facility: facility.clone(),
                    });
                }
            }
            NodeKind::MissingRecipe { recipe, .. } => issues.push(Issue::MissingRecipe {
                node: id,
                recipe: recipe.clone(),
            }),
            NodeKind::OutpostOrder { recipe, item, .. } => {
                let trade = catalog.recipe(recipe.as_str()).filter(|r| r.is_trade);
                match trade {
                    Some(trade) if trade.inputs.first().map(|(i, _)| i) == Some(item) => {
                        if !trade.available_in(&self.region) {
                            issues.push(Issue::RecipeOutsideRegion {
                                node: id,
                                recipe: recipe.clone(),
                            });
                        }
                    }
                    _ => issues.push(Issue::MissingRecipe {
                        node: id,
                        recipe: recipe.clone(),
                    }),
                }
            }
            _ => {
                if let Some(item) = kind.item()
                    && catalog.item(item.as_str()).is_none()
                {
                    issues.push(Issue::UnknownItem {
                        node: id,
                        item: item.clone(),
                    });
                }
            }
        }
    }
}
