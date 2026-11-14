//! App logic shared by the UIs: everything an editor does that isn't drawing. No GUI dependency, and
//! it builds for the web (ADR 0009).

pub mod browser;
pub mod command;
pub mod data;
pub mod document;
pub mod embedded;
pub mod files;
pub mod format;
pub mod i18n;
pub mod messages;
pub mod names;
pub mod picker;
pub mod selection;
pub mod session;
pub mod settings;
pub mod store;
pub mod style;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod throttle;
pub use command::{Chord, ChordKey, Command};
pub use document::{Document, Level};
pub use files::Opened;
pub use selection::Selection;
pub use session::{FileCommand, Outcome, Session, ViewCommand};
