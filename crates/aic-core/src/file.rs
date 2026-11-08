//! Save files (`.aicm`): pretty-printed JSON that refers to game data by ID only, so a file
//! survives data updates.

use std::collections::HashMap;

use aic_data::{Catalog, FacilityId, ItemId, RecipeId, RegionId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::graph::{Graph, Link, Node, NodeId, NodeKind, Pos};

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
}

#[derive(Serialize, Deserialize)]
struct FileGraph {
    format_version: u32,
    data_version: String,
    region: RegionId,
    nodes: Vec<FileNode>,
    links: Vec<FileLink>,
}

#[derive(Serialize, Deserialize)]
struct FileNode {
    id: u32,
    pos: Pos,
    #[serde(flatten)]
    kind: FileNodeKind,
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
}

impl Graph {
    /// The graph as a save file, recording the data version it was made with.
    pub fn to_json(&self, data_version: &str) -> String {
        let ids: HashMap<NodeId, u32> = self.nodes.keys().zip(0..).collect();
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
                    kind: FileNodeKind::from(&node.kind),
                })
                .collect(),
            links: self
                .links
                .values()
                .map(|link| FileLink {
                    from: ids[&link.from],
                    to: ids[&link.to],
                    item: link.item.clone(),
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
        let mut ids = HashMap::new();
        for node in file.nodes {
            let id = graph.nodes.insert(Node {
                kind: node.kind.into_node_kind(catalog),
                pos: node.pos,
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
            Self::Splitter { item } => NodeKind::Splitter { item },
            Self::Merger { item } => NodeKind::Merger { item },
            Self::Storage { item } => NodeKind::Storage { item },
            Self::ControlPort { item, limit } => NodeKind::ControlPort { item, limit },
        }
    }
}
