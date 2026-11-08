//! Graph model, validation, sections and the solver. Has no UI dependency: the UI builds a
//! [`Graph`], calls [`solve`], and renders the [`Solution`].
//!
//! All rates are items per minute.

mod file;
mod graph;
mod solve;
mod validate;

pub use file::{FORMAT_VERSION, FileError, LoadedGraph};
pub use graph::{
    EditError, Graph, Link, LinkError, LinkId, Node, NodeId, NodeKind, Port, PortRef, Pos, Side,
};
pub use solve::{
    SectionResult, SectionStatus, Solution, StorageFlow, ThrottleReason, Totals,
    UndeterminedReason, Warning, solve,
};
pub use validate::Issue;
