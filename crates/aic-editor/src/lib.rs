//! App logic: everything the editor does that isn't drawing. No GUI dependency, and it builds for the
//! web (ADR 0010).

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
pub mod tutorial;
pub use command::{Chord, ChordKey, Command};
pub use document::{Document, Level};
pub use files::Opened;
pub use selection::Selection;
pub use session::{Applied, FileCommand, Outcome, Session, ViewCommand};
