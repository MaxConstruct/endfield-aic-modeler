//! `aic-extract`: turns a folder of the game's exported tables into a data version,
//! `data/<version>.json`, and adds the English names it needs to the app's names table (ADR 0014).
//!
//! ```sh
//! cargo run -p aic-extract -- <Table folder> --version v1.6.0 --previous data/v1.5.3.json
//! ```
//!
//! [`extract`] is the conversion itself, tables in and data file out; [`run`] reads and writes the
//! files around it and checks that what it wrote loads. The file format is `aic-data`'s
//! ([`aic_data::file`]); this crate writes it through those types and knows nothing else of it.

mod error;
mod extract;
mod names;
mod report;
mod tables;

use std::path::{Path, PathBuf};

use aic_data::file::DataFile;
use aic_data::{Catalog, Names};

pub use error::ExtractError;
pub use extract::{
    Extraction, GEN_POWER, ITEM_ALIASES, ITEMLESS_FACILITIES, NATURAL_LIQUIDS, REGIONS, SETTLEMENT,
    TRADE_TIME_S, aliases, extract,
};
pub use names::{NameDifference, NameKind, NamesFile};
pub use report::{AliasKind, DroppedAlias, PriceConflict, Report, SharedName, Summary, Unaliased};
pub use tables::Tables;

/// The command line, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// The folder of exported tables (`…/export_full/game/Table`).
    pub tables: PathBuf,
    /// The data version to write, such as `v1.6.0`.
    pub version: String,
    /// An older data version, in our format: its colours are kept and its IDs get aliases.
    pub previous: Option<PathBuf>,
    pub data_dir: PathBuf,
    pub names: PathBuf,
    /// The text table's language, `EN` for `I18nTextTable_EN.json`.
    pub lang: String,
}

pub const USAGE: &str = "usage: aic-extract <Table folder> --version <version> \
[--previous <data file>] [--data-dir data] [--names crates/aic-editor/i18n/names/en.json] [--lang EN]";

impl Args {
    /// Parses the arguments after the program's name.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, ExtractError> {
        let usage = |message: &str| ExtractError::Usage(format!("{message}\n{USAGE}"));
        let mut tables = None;
        let mut version = None;
        let mut previous = None;
        let mut data_dir = PathBuf::from("data");
        let mut names = PathBuf::from("crates/aic-editor/i18n/names/en.json");
        let mut lang = "EN".to_owned();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = |flag: &str| {
                args.next()
                    .ok_or_else(|| usage(&format!("{flag} needs a value")))
            };
            match arg.as_str() {
                "--version" => version = Some(value("--version")?),
                "--previous" => previous = Some(PathBuf::from(value("--previous")?)),
                "--data-dir" => data_dir = PathBuf::from(value("--data-dir")?),
                "--names" => names = PathBuf::from(value("--names")?),
                "--lang" => lang = value("--lang")?,
                flag if flag.starts_with("--") => {
                    return Err(usage(&format!("unknown flag {flag}")));
                }
                _ if tables.is_none() => tables = Some(PathBuf::from(arg)),
                _ => return Err(usage(&format!("unexpected argument {arg}"))),
            }
        }
        Ok(Self {
            tables: tables.ok_or_else(|| usage("no Table folder"))?,
            version: version.ok_or_else(|| usage("no --version"))?,
            previous,
            data_dir,
            names,
            lang,
        })
    }
}

/// Reads the tables, extracts, checks that the result loads with the names, writes
/// `<data dir>/<version>.json` and the names table, and returns the report to print.
pub fn run(args: &Args) -> Result<String, ExtractError> {
    let tables = Tables::read(&args.tables, &args.lang)?;
    let previous = match &args.previous {
        Some(file) => {
            let bytes = read(file)?;
            Some(DataFile::from_json(&bytes, file).map_err(ExtractError::Previous)?)
        }
        None => None,
    };
    let mut names = NamesFile::read(&args.names)?;
    let Extraction { data, report } =
        extract(&tables, previous.as_ref(), &mut names, &args.version)?;

    let json = data.to_json();
    let names_json = names.to_json();
    let names_file = args.names.display().to_string();
    let catalog = Catalog::from_bytes(
        json.as_bytes(),
        Names {
            file: &names_file,
            json: names_json.as_bytes(),
        },
    )
    .map_err(ExtractError::Invalid)?;

    let out = args.data_dir.join(format!("{}.json", args.version));
    write(&out, &json)?;
    write(&args.names, &names_json)?;
    let mut text = format!("wrote {} and {}\n", out.display(), args.names.display());
    text += &Summary {
        data: &data,
        report: &report,
    }
    .to_string();
    for warning in catalog.warnings() {
        text += &format!("load warning: {warning}\n");
    }
    Ok(text)
}

fn read(file: &Path) -> Result<Vec<u8>, ExtractError> {
    std::fs::read(file).map_err(|source| ExtractError::Read {
        file: file.to_path_buf(),
        source,
    })
}

fn write(file: &Path, text: &str) -> Result<(), ExtractError> {
    std::fs::write(file, text).map_err(|source| ExtractError::Write {
        file: file.to_path_buf(),
        source,
    })
}
