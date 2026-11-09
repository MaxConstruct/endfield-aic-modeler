//! Graph model, validation, sections and the solver. Has no UI dependency: the UI builds a
//! [`Graph`], calls [`solve`], and renders the [`Solution`].
//!
//! All rates are items per minute.

mod file;
mod graph;
mod group;
mod solve;
mod validate;

pub use file::{CanvasView, FORMAT_VERSION, FileError, LoadedGraph};
pub use graph::{
    EditError, Graph, GroupId, Link, LinkError, LinkId, Node, NodeId, NodeKind, Port, PortRef, Pos,
    Side,
};
pub use group::{Group, GroupMode, Member};
pub use solve::{
    RoundUp, SectionResult, SectionStatus, Solution, StorageFlow, ThrottleReason, Totals,
    UndeterminedReason, Warning, check_resources, solve,
};
pub use validate::Issue;
