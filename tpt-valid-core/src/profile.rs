//! Column profiling for CSV data (Phase 13): null rates, distinct counts and
//! type mixes per column, plus tighter-schema suggestions (enum candidates,
//! `null` alternatives, constant columns).
//!
//! Bounded memory: at most [`DISTINCT_SAMPLE_CAP`] distinct values are kept
//! per column; counts remain exact.

use std::collections::HashMap;

use serde::Serialize;

use crate::csv::{CsvDialect, CsvReader};

/// Maximum distinct values tracked per column (counts stay exact).
pub const DISTINCT_SAMPLE_CAP: usize = 100;

/// How many rows are profiled by default.
pub const PROFILE_SAMPLE_ROWS: usize = 10_000;

/// Profile of one CSV column.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ColumnProfile {
    /// Column name (header value, or `column_N` for headerless input).
    pub name: String,
    /// Rows sampled.
    pub rows: usize,
    /// Cells that parsed to nothing (empty strings).
    pub empty: usize,
    /// Distinct non-empty values (exact count; values sampled up to
    /// [`DISTINCT_SAMPLE_CAP`]).
    pub distinct: usize,
    /// The sampled distinct values (bounded; sorted).
    pub distinct_values: Vec<String>,
    /// Observed shape of the values.
    pub integers: usize,
    /// Cells parsing as non-integer floats.
    pub floats: usize,
    /// Cells parsing as true/false.
    pub booleans: usize,
    /// Cells that are none of the above (free text).
    pub other: usize,
}

impl ColumnProfile {
    /// Share of rows whose cell was empty, in `0.0..=1.0`.
    pub fn empty_rate(&self) -> f64 {
        if self.rows == 0 {
            0.0
        } else {
            self.empty as f64 / self.rows as f64
        }
    }

    /// Tighter-schema suggestions for this column.
    pub fn suggestions(&self) -> Vec<String> {
        let mut out = Vec::new();
        let filled = self.rows - self.empty;
        if filled == 0 {
            return out;
        }
        if self.distinct == 1 {
            out.push(format!(
                "column \"{}\" is constant (\"{}\"); consider a `const` schema",
                self.name,
                self.distinct_values
                    .first()
                    .map(String::as_str)
                    .unwrap_or("")
            ));
        } else if self.distinct >= 2 && self.distinct <= 8 {
            out.push(format!(
                "column \"{}\" has only {distinct} distinct values; consider an `enum`: {:?}",
                self.name,
                self.distinct_values,
                distinct = self.distinct
            ));
        }
        if self.empty > 0 && self.empty < self.rows {
            out.push(format!(
                "column \"{}\" is {empty_rate:.0}% empty; add \"null\" to its type union (or make it optional)",
                self.name,
                empty_rate = self.empty_rate() * 100.0
            ));
        }
        if self.other == 0 && self.integers > 0 && self.floats == 0 && self.booleans == 0 {
            out.push(format!(
                "column \"{}\" is entirely integers; declare \"type\": \"integer\" so schema-driven coercion applies",
                self.name
            ));
        }
        out
    }
}

/// Profile a CSV source, sampling up to `sample_rows` data rows.
pub fn profile_csv<R: std::io::BufRead>(
    reader: R,
    dialect: &CsvDialect,
    sample_rows: usize,
) -> Result<Vec<ColumnProfile>, crate::csv::CsvParseError> {
    let mut csv = CsvReader::with_dialect(reader, dialect.clone());
    let headers: Vec<String> = if dialect.has_headers {
        match csv.next_record()? {
            Some(h) => h.fields,
            None => return Ok(Vec::new()),
        }
    } else {
        Vec::new()
    };

    let mut profiles: Vec<ColumnProfile> = Vec::new();
    // Exact distinct counts with a bounded value sample per column.
    let mut seen: Vec<std::collections::HashSet<String>> = Vec::new();
    let mut rows = 0usize;
    while rows < sample_rows {
        let Some(record) = csv.next_record()? else {
            break;
        };
        rows += 1;
        while seen.len() < record.fields.len() {
            seen.push(std::collections::HashSet::new());
        }
        while profiles.len() < record.fields.len() {
            let name = profiles.len().to_string();
            profiles.push(ColumnProfile {
                name: if dialect.has_headers {
                    headers.get(profiles.len()).cloned().unwrap_or(name)
                } else {
                    format!("column_{}", profiles.len())
                },
                ..Default::default()
            });
        }
        for (i, cell) in record.fields.iter().enumerate() {
            let profile = &mut profiles[i];
            profile.rows += 1;
            if cell.is_empty() {
                profile.empty += 1;
                continue;
            }
            if cell.parse::<i64>().is_ok() {
                profile.integers += 1;
            } else if cell.parse::<f64>().is_ok() {
                profile.floats += 1;
            } else if cell.eq_ignore_ascii_case("true") || cell.eq_ignore_ascii_case("false") {
                profile.booleans += 1;
            } else {
                profile.other += 1;
            }
            let column_seen = &mut seen[i];
            if column_seen.insert(cell.clone()) && column_seen.len() <= DISTINCT_SAMPLE_CAP {
                profile.distinct_values.push(cell.clone());
            }
        }
    }
    for (profile, column_seen) in profiles.iter_mut().zip(&seen) {
        profile.distinct = column_seen.len();
        profile.distinct_values.sort();
    }
    Ok(profiles)
}

/// Column profiles indexed by name (helper for callers).
pub fn profiles_by_name(profiles: &[ColumnProfile]) -> HashMap<&str, &ColumnProfile> {
    profiles.iter().map(|p| (p.name.as_str(), p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn profile(data: &str) -> Vec<ColumnProfile> {
        profile_csv(
            Cursor::new(data),
            &CsvDialect::default(),
            PROFILE_SAMPLE_ROWS,
        )
        .unwrap()
    }

    #[test]
    fn profiles_basic_columns() {
        let profiles = profile("name,qty,flag\nAlice,1,true\nBob,2,false\n,3,true\n");
        assert_eq!(profiles.len(), 3);
        let name = &profiles[0];
        assert_eq!(name.name, "name");
        assert_eq!(name.rows, 3);
        assert_eq!(name.empty, 1);
        assert_eq!(name.distinct, 2);
        assert!(name.empty_rate() > 0.3 && name.empty_rate() < 0.34);

        let qty = &profiles[1];
        assert_eq!(qty.integers, 3);
        assert_eq!(qty.distinct, 3);
    }

    #[test]
    fn suggestions_cover_enum_null_and_const() {
        let profiles = profile("status,note,qty\nnew,x,1\nnew,y,2\nnew,z,3\n");
        let status = &profiles[0];
        assert_eq!(status.distinct, 1);
        let suggestions = status.suggestions().join("\n");
        assert!(suggestions.contains("constant"), "{suggestions}");

        let note = &profiles[1];
        assert!(note.suggestions().iter().any(|s| s.contains("enum")));

        let qty = &profiles[2];
        assert!(qty
            .suggestions()
            .iter()
            .any(|s| s.contains("type\": \"integer\"")));
    }
}
