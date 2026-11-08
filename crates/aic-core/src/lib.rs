//! Graph model and validation; the solver comes in M3. Has no UI dependency.
//!
//! All rates are items per minute.

mod file;
mod graph;
mod validate;

pub use file::{FORMAT_VERSION, FileError, LoadedGraph};
pub use graph::{
    EditError, Graph, Link, LinkError, LinkId, Node, NodeId, NodeKind, Port, PortRef, Pos, Side,
};
pub use validate::Issue;
