//! What the extractor found worth saying, printed after a run.

use std::fmt;

use aic_data::file::DataFile;

use crate::names::NameDifference;

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Names the names table lacked and got.
    pub names_added: usize,
    /// Names the table has, which the game calls something else. The table's are kept.
    pub name_differences: Vec<NameDifference>,
    /// Names the table had, replaced: a facility's by the game's name for its building, and a
    /// recipe's by ours where we name it (`Packaged …`, `Unpackage …`, one of several the game
    /// names alike).
    pub names_changed: Vec<NameDifference>,
    /// IDs the game has no name for (`item x`, `recipe y`, …).
    pub missing_names: Vec<String>,
    /// Items two outposts of one region pay differently for.
    pub price_conflicts: Vec<PriceConflict>,
    /// Recipes of the previous version that got no alias.
    pub unaliased: Vec<Unaliased>,
    /// Names several recipes still have after telling them apart by input and gas environment.
    pub shared_names: Vec<SharedName>,
    /// Recipes whose items share no region; they are listed in every region.
    pub regionless: Vec<String>,
    /// The sewage plant's inlets and the outlet's seconds per craft, as worked out.
    pub sewage_export: Option<(usize, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceConflict {
    pub region: String,
    pub item: String,
    pub offers: Vec<f64>,
    pub kept: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedName {
    pub name: String,
    /// The recipes' IDs.
    pub recipes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unaliased {
    pub id: String,
    pub reason: String,
}

/// The report with the data it describes, for printing.
pub struct Summary<'a> {
    pub data: &'a DataFile,
    pub report: &'a Report,
}

impl fmt::Display for Summary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (data, report) = (self.data, self.report);
        writeln!(
            f,
            "{} items, {} facilities, {} recipes, {} item aliases, {} recipe aliases",
            data.items.len(),
            data.facilities.len(),
            data.recipes.len(),
            data.aliases.items.len(),
            data.aliases.recipes.len(),
        )?;
        writeln!(
            f,
            "belt {}/min, pipe {}/min",
            data.belt_per_min, data.pipe_per_min
        )?;
        if let Some((inlets, time_s)) = report.sewage_export {
            writeln!(
                f,
                "sewage outlet: {inlets} inlets per plant, {time_s} s per craft"
            )?;
        }
        writeln!(f, "names added: {}", report.names_added)?;
        section(
            f,
            "names that differ from the game's (ours kept)",
            &report.name_differences,
            |d| {
                format!(
                    "{:?} {}: ours {:?}, game {:?}",
                    d.kind, d.id, d.ours, d.game
                )
            },
        )?;
        section(
            f,
            "names replaced (a building's by the game's, a recipe's by ours)",
            &report.names_changed,
            |d| format!("{}: was {:?}, now {:?}", d.id, d.ours, d.game),
        )?;
        section(
            f,
            "no name in the game",
            &report.missing_names,
            Clone::clone,
        )?;
        section(
            f,
            "outposts that disagree on a price (the larger kept)",
            &report.price_conflicts,
            |c| format!("{} {}: {:?}, kept {}", c.region, c.item, c.offers, c.kept),
        )?;
        section(
            f,
            "names several recipes still share",
            &report.shared_names,
            |s| format!("{:?}: {}", s.name, s.recipes.join(", ")),
        )?;
        section(
            f,
            "recipes whose items share no region",
            &report.regionless,
            Clone::clone,
        )?;
        section(
            f,
            "previous recipes without an alias",
            &report.unaliased,
            |u| format!("{}: {}", u.id, u.reason),
        )
    }
}

fn section<T>(
    f: &mut fmt::Formatter<'_>,
    title: &str,
    entries: &[T],
    line: impl Fn(&T) -> String,
) -> fmt::Result {
    if entries.is_empty() {
        return Ok(());
    }
    writeln!(f, "{title} ({}):", entries.len())?;
    for entry in entries {
        writeln!(f, "  {}", line(entry))?;
    }
    Ok(())
}
