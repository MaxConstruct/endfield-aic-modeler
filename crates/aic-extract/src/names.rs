//! The app's names table, `crates/aic-editor/i18n/names/<lang>.json` (ADR 0012), read and written
//! in its own key order so new names are appended and nothing else moves.

use std::path::Path;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::error::ExtractError;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamesFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<serde_json::Value>,
    #[serde(default)]
    pub categories: IndexMap<String, String>,
    /// Items and facilities.
    #[serde(default)]
    pub items: IndexMap<String, String>,
    #[serde(default)]
    pub recipes: IndexMap<String, String>,
    /// Regions.
    #[serde(default)]
    pub locations: IndexMap<String, String>,
    /// How the file it was read from ends its lines, kept when it is written back.
    #[serde(skip)]
    style: Style,
}

/// Line endings (a Windows checkout may have made them CRLF), and whether the file ends with one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Style {
    crlf: bool,
    final_newline: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            crlf: false,
            final_newline: true,
        }
    }
}

/// Which part of a names table a name goes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    Category,
    /// An item or a facility.
    Item,
    Recipe,
    Region,
}

/// A name the table already has, which the game calls something else. The table's is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameDifference {
    pub kind: NameKind,
    pub id: String,
    pub ours: String,
    pub game: String,
}

impl NamesFile {
    pub fn read(file: &Path) -> Result<Self, ExtractError> {
        let bytes = std::fs::read(file).map_err(|source| ExtractError::Read {
            file: file.to_path_buf(),
            source,
        })?;
        Self::from_json(&bytes, file)
    }

    pub fn from_json(bytes: &[u8], file: &Path) -> Result<Self, ExtractError> {
        let mut names: Self =
            serde_json::from_slice(bytes).map_err(|source| ExtractError::Json {
                file: file.to_path_buf(),
                source,
            })?;
        names.style = Style {
            crlf: bytes.windows(2).any(|pair| pair == b"\r\n"),
            final_newline: bytes.ends_with(b"\n"),
        };
        Ok(names)
    }

    /// Pretty-printed, with the line endings of the file it was read from.
    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a names table has string keys");
        if self.style.final_newline {
            json.push('\n');
        }
        if self.style.crlf {
            json = json.replace('\n', "\r\n");
        }
        json
    }

    fn table(&mut self, kind: NameKind) -> &mut IndexMap<String, String> {
        match kind {
            NameKind::Category => &mut self.categories,
            NameKind::Item => &mut self.items,
            NameKind::Recipe => &mut self.recipes,
            NameKind::Region => &mut self.locations,
        }
    }

    /// Gives `id` the name `name`, in the place its old name had or else at the end. Whether a
    /// name was added is the `bool`; a different name it replaced is returned as the difference.
    pub fn set(&mut self, kind: NameKind, id: &str, name: &str) -> (bool, Option<NameDifference>) {
        match self.table(kind).insert(id.to_owned(), name.to_owned()) {
            None => (true, None),
            Some(ours) if ours == name => (false, None),
            Some(ours) => (
                false,
                Some(NameDifference {
                    kind,
                    id: id.to_owned(),
                    ours,
                    game: name.to_owned(),
                }),
            ),
        }
    }

    /// Adds `name` for `id` if the table has none. An existing name stays as it is; if it isn't
    /// `name`, the difference is returned. Whether a name was added is the `bool`.
    pub fn add(&mut self, kind: NameKind, id: &str, name: &str) -> (bool, Option<NameDifference>) {
        let table = self.table(kind);
        match table.get(id) {
            None => {
                table.insert(id.to_owned(), name.to_owned());
                (true, None)
            }
            Some(ours) if ours == name => (false, None),
            Some(ours) => (
                false,
                Some(NameDifference {
                    kind,
                    id: id.to_owned(),
                    ours: ours.clone(),
                    game: name.to_owned(),
                }),
            ),
        }
    }
}
