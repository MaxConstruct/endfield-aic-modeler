//! Save files (`.aicm`): pretty-printed JSON that refers to game data by ID only, so a file
//! survives data updates.

use std::collections::HashMap;

use aic_data::{Catalog, FacilityId, ItemId, RecipeId, RegionId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::graph::{Graph, GroupId, Link, Node, NodeId, NodeKind, Pos};
use crate::group::{Group, GroupMode};

/// The save file format this build writes and the newest it reads.
pub const FORMAT_VERSION: u32 = 1;

/// A graph read from a file, with the data version it was made with. If that isn't the active data
/// version, the UI shows a banner; the graph is still loaded by ID.
#[derive(Debug, Clone)]
pub struct LoadedGraph {
    pub graph: Graph,
    pub data_version: String,
}

#[derive(Debug, Error)]
pub enum FileError {
    #[error("at `{path}`: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("the file has format version {found}; this build reads up to {supported}")]
    UnsupportedFormat { found: u32, supported: u32 },
    #[error("two nodes have the id {id}")]
    DuplicateNode { id: u32 },
    #[error("link {index} refers to node {id}, which isn't in the file")]
    UnknownNode { index: usize, id: u32 },
    #[error("group {id} is used but isn't in the file, or two groups have that id")]
    UnknownGroup { id: u32 },
    #[error("group {id} is inside itself")]
    GroupCycle { id: u32 },
}

#[derive(Serialize, Deserialize)]
struct FileGraph {
    format_version: u32,
    data_version: String,
    region: RegionId,
    nodes: Vec<FileNode>,
    links: Vec<FileLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<FileGroup>,
}

#[derive(Serialize, Deserialize)]
struct FileNode {
    id: u32,
    pos: Pos,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<u32>,
    #[serde(flatten)]
    kind: FileNodeKind,
}

#[derive(Serialize, Deserialize)]
struct FileGroup {
    id: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<u32>,
    #[serde(default)]
    collapsed: bool,
    pos: Pos,
    /// Present for a copies group; its value is the copies limit, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    copies: Option<FileCopies>,
}

#[derive(Serialize, Deserialize)]
struct FileCopies {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    limit: Option<f64>,
}

/// `MissingRecipe` is written back as the recipe it was, so it isn't a kind of its own here.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum FileNodeKind {
    Recipe {
        recipe: RecipeId,
        facility: FacilityId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<f64>,
    },
    Source {
        item: ItemId,
        limit: f64,
    },
    Sink {
        item: ItemId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<f64>,
    },
    OutpostOrder {
        recipe: RecipeId,
        item: ItemId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<f64>,
    },
    Splitter {
        item: Option<ItemId>,
    },
    Merger {
        item: Option<ItemId>,
    },
    Storage {
        item: Option<ItemId>,
    },
    ControlPort {
        item: Option<ItemId>,
        limit: f64,
    },
}

#[derive(Serialize, Deserialize)]
struct FileLink {
    from: u32,
    to: u32,
    item: ItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    out_share: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    in_share: Option<f64>,
}

impl Graph {
    /// The graph as a save file, recording the data version it was made with.
    pub fn to_json(&self, data_version: &str) -> String {
        let ids: HashMap<NodeId, u32> = self.nodes.keys().zip(0..).collect();
        let group_ids: HashMap<GroupId, u32> = self.groups.keys().zip(0..).collect();
        let file = FileGraph {
            format_version: FORMAT_VERSION,
            data_version: data_version.to_owned(),
            region: self.region.clone(),
            nodes: self
                .nodes
                .iter()
                .map(|(id, node)| FileNode {
                    id: ids[&id],
                    pos: node.pos,
                    group: node.group.map(|g| group_ids[&g]),
                    kind: FileNodeKind::from(&node.kind),
                })
                .collect(),
            groups: self
                .groups
                .iter()
                .map(|(id, group)| FileGroup {
                    id: group_ids[&id],
                    name: group.name.clone(),
                    parent: group.parent.map(|p| group_ids[&p]),
                    collapsed: group.collapsed,
                    pos: group.pos,
                    copies: match group.mode {
                        GroupMode::Folder => None,
                        GroupMode::Copies { limit } => Some(FileCopies { limit }),
                    },
                })
                .collect(),
            links: self
                .links
                .values()
                .map(|link| FileLink {
                    from: ids[&link.from],
                    to: ids[&link.to],
                    item: link.item.clone(),
                    out_share: link.out_share,
                    in_share: link.in_share,
                })
                .collect(),
        };
        serde_json::to_string_pretty(&file).expect("a graph has only string keys and plain values")
    }

    /// Reads a save file. A recipe or facility the catalog doesn't know becomes a
    /// [`NodeKind::MissingRecipe`] node rather than an error. Links are not checked here; call
    /// [`Graph::validate`].
    pub fn from_json(catalog: &Catalog, json: &str) -> Result<LoadedGraph, FileError> {
        let mut de = serde_json::Deserializer::from_str(json);
        let json_error = |path: String, source| FileError::Json { path, source };
        let file: FileGraph = serde_path_to_error::deserialize(&mut de)
            .map_err(|e| json_error(e.path().to_string(), e.into_inner()))?;
        de.end().map_err(|e| json_error(".".to_owned(), e))?;
        if file.format_version > FORMAT_VERSION {
            return Err(FileError::UnsupportedFormat {
                found: file.format_version,
                supported: FORMAT_VERSION,
            });
        }

        let mut graph = Graph::new(file.region);
        let mut group_ids = HashMap::new();
        for group in &file.groups {
            let id = graph.groups.insert(Group {
                name: group.name.clone(),
                parent: None,
                collapsed: group.collapsed,
                pos: group.pos,
                mode: match &group.copies {
                    None => GroupMode::Folder,
                    Some(copies) => GroupMode::Copies {
                        limit: copies.limit,
                    },
                },
            });
            if group_ids.insert(group.id, id).is_some() {
                return Err(FileError::UnknownGroup { id: group.id });
            }
        }
        let group = |id: Option<u32>| -> Result<Option<GroupId>, FileError> {
            id.map(|id| {
                group_ids
                    .get(&id)
                    .copied()
                    .ok_or(FileError::UnknownGroup { id })
            })
            .transpose()
        };
        for file_group in &file.groups {
            let parent = group(file_group.parent)?;
            graph.groups[group_ids[&file_group.id]].parent = parent;
        }
        // Walking up from any group must end, or every lookup of what is inside what would loop.
        for file_group in &file.groups {
            let mut current = graph.groups[group_ids[&file_group.id]].parent;
            for _ in 0..=file.groups.len() {
                current = current.and_then(|g| graph.groups[g].parent);
            }
            if current.is_some() {
                return Err(FileError::GroupCycle { id: file_group.id });
            }
        }
        let mut ids = HashMap::new();
        for node in file.nodes {
            let id = graph.nodes.insert(Node {
                kind: node.kind.into_node_kind(catalog),
                pos: node.pos,
                group: group(node.group)?,
            });
            if ids.insert(node.id, id).is_some() {
                return Err(FileError::DuplicateNode { id: node.id });
            }
        }
        for (index, link) in file.links.into_iter().enumerate() {
            let node = |id: u32| {
                ids.get(&id)
                    .copied()
                    .ok_or(FileError::UnknownNode { index, id })
            };
            graph.links.insert(Link {
                from: node(link.from)?,
                to: node(link.to)?,
                item: link.item,
                // A share that isn't a positive number can't lock a ratio; the link stays free.
                out_share: link.out_share.filter(|s| s.is_finite() && *s > 0.0),
                in_share: link.in_share.filter(|s| s.is_finite() && *s > 0.0),
            });
        }
        Ok(LoadedGraph {
            graph,
            data_version: file.data_version,
        })
    }
}

impl From<&NodeKind> for FileNodeKind {
    fn from(kind: &NodeKind) -> Self {
        match kind.clone() {
            NodeKind::Recipe {
                recipe,
                facility,
                limit,
            }
            | NodeKind::MissingRecipe {
                recipe,
                facility,
                limit,
            } => Self::Recipe {
                recipe,
                facility,
                limit,
            },
            NodeKind::Source { item, limit } => Self::Source { item, limit },
            NodeKind::Sink { item, limit } => Self::Sink { item, limit },
            NodeKind::OutpostOrder {
                recipe,
                item,
                limit,
            } => Self::OutpostOrder {
                recipe,
                item,
                limit,
            },
            NodeKind::Splitter { item } => Self::Splitter { item },
            NodeKind::Merger { item } => Self::Merger { item },
            NodeKind::Storage { item } => Self::Storage { item },
            NodeKind::ControlPort { item, limit } => Self::ControlPort { item, limit },
        }
    }
}

impl FileNodeKind {
    fn into_node_kind(self, catalog: &Catalog) -> NodeKind {
        match self {
            Self::Recipe {
                recipe,
                facility,
                limit,
            } => {
                let known = catalog.recipe(recipe.as_str()).is_some()
                    && catalog.facility(facility.as_str()).is_some();
                if known {
                    NodeKind::Recipe {
                        recipe,
                        facility,
                        limit,
                    }
                } else {
                    NodeKind::MissingRecipe {
                        recipe,
                        facility,
                        limit,
                    }
                }
            }
            Self::Source { item, limit } => NodeKind::Source { item, limit },
            Self::Sink { item, limit } => NodeKind::Sink { item, limit },
            Self::OutpostOrder {
                recipe,
                item,
                limit,
            } => NodeKind::OutpostOrder {
                recipe,
                item,
                limit,
            },
            Self::Splitter { item } => NodeKind::Splitter { item },
            Self::Merger { item } => NodeKind::Merger { item },
            Self::Storage { item } => NodeKind::Storage { item },
            Self::ControlPort { item, limit } => NodeKind::ControlPort { item, limit },
        }
    }
}
