//! `aic-extract <Table folder> --version v1.6.0 [--previous data/v1.5.3.json] [--data-dir data]
//! [--names crates/aic-editor/i18n/names/en.json] [--lang EN]`: see the library's docs and
//! `data/README.md`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let result =
        aic_extract::Args::parse(std::env::args().skip(1)).and_then(|args| aic_extract::run(&args));
    match result {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("aic-extract: {error}");
            ExitCode::FAILURE
        }
    }
}
