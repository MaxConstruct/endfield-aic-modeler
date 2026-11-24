//! Save files (`.aicm`): pretty-printed JSON that refers to game data by ID only, so a file
//! survives data updates.

use std::collections::HashMap;

use aic_data::{Catalog, FacilityId, ItemId, RecipeId, RegionId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::graph::{Graph, Link, Node, NodeId, NodeKind, Pos, Side, WorkspaceId};
use crate::workspace::Workspace;

/// The save file format this build writes and the newest it reads.
///
/// Version 1 had groups: a link could join nodes in different groups. Version 2 has workspaces,
/// whose links cross their edge only through ports; reading a version 1 file sends its crossing
/// links through new ports.
pub const FORMAT_VERSION: u32 = 2;

/// A graph read from a file, with the data version it was made with. If that isn't the active data
/// version, the UI shows a banner; the graph is still loaded by ID.
#[derive(Debug, Clone)]
pub struct LoadedGraph {
    pub graph: Graph,
    pub data_version: String,
    /// Where the canvas was looking when the file was saved, if it says.
    pub view: Option<CanvasView>,
}

/// The canvas camera saved with a graph: the graph point at the top-left of the canvas, and the zoom.
/// The solver ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasView {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
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
    #[error("workspace {id} is used but isn't in the file, or two workspaces have that id")]
    UnknownWorkspace { id: u32 },
    #[error("workspace {id} is inside itself")]
    WorkspaceCycle { id: u32 },
}

#[derive(Serialize, Deserialize)]
struct FileGraph {
    format_version: u32,
    data_version: String,
    region: RegionId,
    nodes: Vec<FileNode>,
    links: Vec<FileLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "groups")]
    workspaces: Vec<FileWorkspace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    view: Option<CanvasView>,
}

#[derive(Serialize, Deserialize)]
struct FileNode {
    id: u32,
    pos: Pos,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "group")]
    workspace: Option<u32>,
    #[serde(flatten)]
    kind: FileNodeKind,
}

/// A version 1 group also had `collapsed` and `copies`; they are ignored.
#[derive(Serialize, Deserialize)]
struct FileWorkspace {
    id: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<u32>,
    pos: Pos,
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
    WorkspacePort {
        side: FileSide,
        item: Option<ItemId>,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum FileSide {
    Input,
    Output,
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
        self.to_json_with_view(data_version, None)
    }

    /// The graph as a save file, with where the canvas was looking.
    pub fn to_json_with_view(&self, data_version: &str, view: Option<CanvasView>) -> String {
        let ids: HashMap<NodeId, u32> = self.nodes.keys().zip(0..).collect();
        let workspace_ids: HashMap<WorkspaceId, u32> = self.workspaces.keys().zip(0..).collect();
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
                    workspace: node.workspace.map(|w| workspace_ids[&w]),
                    kind: FileNodeKind::from(&node.kind),
                })
                .collect(),
            workspaces: self
                .workspaces
                .iter()
                .map(|(id, workspace)| FileWorkspace {
                    id: workspace_ids[&id],
                    name: workspace.name.clone(),
                    parent: workspace.parent.map(|p| workspace_ids[&p]),
                    pos: workspace.pos,
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
            view,
        };
        serde_json::to_string_pretty(&file).expect("a graph has only string keys and plain values")
    }

    /// Reads a save file. IDs the catalog has aliases for become the ones they stand for
    /// ([`Graph::rebind`]); a recipe or facility the catalog doesn't know becomes a
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
        let mut workspace_ids = HashMap::new();
        for workspace in &file.workspaces {
            let id = graph.workspaces.insert(Workspace {
                name: workspace.name.clone(),
                parent: None,
                pos: workspace.pos,
            });
            if workspace_ids.insert(workspace.id, id).is_some() {
                return Err(FileError::UnknownWorkspace { id: workspace.id });
            }
        }
        let workspace = |id: Option<u32>| -> Result<Option<WorkspaceId>, FileError> {
            id.map(|id| {
                workspace_ids
                    .get(&id)
                    .copied()
                    .ok_or(FileError::UnknownWorkspace { id })
            })
            .transpose()
        };
        for file_workspace in &file.workspaces {
            let parent = workspace(file_workspace.parent)?;
            graph.workspaces[workspace_ids[&file_workspace.id]].parent = parent;
        }
        // Walking up from any workspace must end, or every lookup of what is inside what would
        // loop.
        for file_workspace in &file.workspaces {
            let mut current = graph.workspaces[workspace_ids[&file_workspace.id]].parent;
            for _ in 0..=file.workspaces.len() {
                current = current.and_then(|w| graph.workspaces[w].parent);
            }
            if current.is_some() {
                return Err(FileError::WorkspaceCycle {
                    id: file_workspace.id,
                });
            }
        }
        let mut ids = HashMap::new();
        for node in file.nodes {
            let id = graph.nodes.insert(Node {
                kind: node.kind.into_node_kind(catalog),
                pos: node.pos,
                workspace: workspace(node.workspace)?,
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
        if file.format_version < 2 {
            graph.route_crossing_links();
        }
        // IDs of an older data version that this one has aliases for.
        graph.rebind(catalog);
        // A view that isn't finite or has no zoom is dropped rather than shown.
        let view = file
            .view
            .filter(|v| v.x.is_finite() && v.y.is_finite() && v.zoom.is_finite() && v.zoom > 0.0);
        Ok(LoadedGraph {
            graph,
            data_version: file.data_version,
            view,
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
            NodeKind::WorkspacePort { side, item } => Self::WorkspacePort {
                side: match side {
                    Side::Input => FileSide::Input,
                    Side::Output => FileSide::Output,
                },
                item,
            },
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
            Self::WorkspacePort { side, item } => NodeKind::WorkspacePort {
                side: match side {
                    FileSide::Input => Side::Input,
                    FileSide::Output => Side::Output,
                },
                item,
            },
        }
    }
}
