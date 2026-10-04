//! `validex profile <data.csv>` — column profiling: null rates, distinct
//! counts, observed types, and tighter-schema suggestions (enum candidates,
//! constant columns, null unions, schema-driven coercion hints).

use std::io::Write;

use tpt_valid_core::{profile_csv, CsvDialect};

use crate::Args;

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let args = Args::parse(args)?;
    let data_path = args
        .positional
        .first()
        .ok_or("profile requires a <data> path")?;
    let rows = args
        .get("rows")
        .map(|v| v.parse::<usize>().map_err(|_| "invalid --rows"))
        .transpose()?
        .unwrap_or(tpt_valid_core::profile::PROFILE_SAMPLE_ROWS);
    let dialect = CsvDialect {
        delimiter: crate::check_delimiter(args.get("delimiter"))?,
        has_headers: !args.has("no-headers"),
        ..CsvDialect::default()
    };

    let file =
        std::fs::File::open(data_path).map_err(|e| format!("cannot read {data_path}: {e}"))?;
    let profiles = profile_csv(std::io::BufReader::new(file), &dialect, rows)
        .map_err(|e| format!("CSV error: {e}"))?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(
        out,
        "{:<24} {:>6} {:>6} {:>8}  {:<10}",
        "column", "rows", "empty", "distinct", "types"
    );
    for profile in &profiles {
        let mut types = Vec::new();
        if profile.integers > 0 {
            types.push(format!("int×{}", profile.integers));
        }
        if profile.floats > 0 {
            types.push(format!("float×{}", profile.floats));
        }
        if profile.booleans > 0 {
            types.push(format!("bool×{}", profile.booleans));
        }
        if profile.other > 0 {
            types.push(format!("text×{}", profile.other));
        }
        let _ = writeln!(
            out,
            "{:<24} {:>6} {:>6} {:>8}  {:<10}",
            profile.name,
            profile.rows,
            profile.empty,
            profile.distinct,
            types.join(" "),
        );
    }
    let mut any = false;
    for profile in &profiles {
        for suggestion in profile.suggestions() {
            any = true;
            let _ = writeln!(out, "hint: {suggestion}");
        }
    }
    if !any {
        let _ = writeln!(out, "no tighter-schema suggestions");
    }
    Ok(())
}
