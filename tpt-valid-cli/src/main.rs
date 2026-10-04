//! # validex — the tpt-validex command-line interface
//!
//! Validate data files against JSON Schema (Draft 2020-12 subset) from the
//! terminal, infer starter schemas from sample data, diff schema versions
//! for breaking changes, and watch files for continuous validation.
//!
//! ```text
//! validex check <schema> <data.{json,jsonl,csv}> [options]
//! validex infer <data.{json,jsonl,csv}> [--output file]
//! validex diff <old.json> <new.json> [--format text|json]
//! validex watch [--config .validex.toml]
//! ```
//!
//! Exit codes: 0 all valid / compatible · 1 validation failures or breaking
//! changes · 2 usage, I/O or schema errors.
//!
//! See `LICENSE-MIT` and `LICENSE-APACHE` for licensing terms.

mod check;
mod diff;
mod glob;
mod infer;
mod memcheck;
mod profile;
mod toml_min;
mod watch;

use std::process::ExitCode;

const USAGE: &str = "\
validex — universal data validation from the command line

USAGE:
    validex check <schema> <data.{json,jsonl,csv}> [options]
    validex infer <data.{json,jsonl,csv}> [--output <file>]
    validex diff <old-schema.json> <new-schema.json> [--format text|json]
    validex profile <data.csv> [--rows <N>] [--delimiter <c>] [--no-headers]
    validex watch [--config <file>]

CHECK OPTIONS:
    --errors <file>      Write invalid records as JSONL ({\"line\", \"row\", \"errors\"})
    --valid <file>       Write valid records (JSONL for json/jsonl, CSV for csv)
    --fail-fast          Stop at the first failing record
    --max-errors <N>     Cap collected errors per record (default 1000)
    --format <fmt>       Output format: text (default) | json | junit | sarif
    --delimiter <c>      CSV field delimiter (default ',')
    --no-headers         Treat the first CSV row as data
    --registry <file>    JSON object mapping URIs to schema documents for
                         cross-file $ref resolution
    --quiet              Only print the summary line

INFER OPTIONS:
    --output <file>      Write the inferred schema to a file (default stdout)
    --delimiter <c>      CSV field delimiter (default ',')
    --no-headers         Treat the first CSV row as data

DIFF OPTIONS:
    --format <fmt>       Output format: text (default) | json

WATCH OPTIONS:
    --config <file>      Config file (default .validex.toml)

MISC:
    --help | -h          Show this help
    --version            Show version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("--version") => {
            println!("validex {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("check") => match check::run(&args[1..]) {
            Ok(failures) => {
                if failures {
                    ExitCode::from(1)
                } else {
                    ExitCode::SUCCESS
                }
            }
            Err(e) => {
                eprintln!("validex: {e}");
                ExitCode::from(2)
            }
        },
        Some("infer") => match infer::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("validex: {e}");
                ExitCode::from(2)
            }
        },
        Some("diff") => match diff::run(&args[1..]) {
            Ok(compatible) => {
                if compatible {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                }
            }
            Err(e) => {
                eprintln!("validex: {e}");
                ExitCode::from(2)
            }
        },
        Some("memcheck") => match memcheck::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("validex: {e}");
                ExitCode::from(2)
            }
        },
        Some("profile") => match profile::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("validex: {e}");
                ExitCode::from(2)
            }
        },
        Some("watch") => match watch::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("validex: {e}");
                ExitCode::from(2)
            }
        },
        Some(other) => {
            eprintln!("validex: unknown command \"{other}\"\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Small hand-rolled flag parser shared by the subcommands: positional
/// arguments stay in order, `--flag value` / `--flag=value` pairs are
/// collected into a map.
pub(crate) struct Args {
    pub positional: Vec<String>,
    flags: std::collections::HashMap<String, String>,
}

impl Args {
    /// Flags that take a value (`--flag value` or `--flag=value`); every
    /// other `--flag` is boolean.
    const VALUE_FLAGS: &'static [&'static str] = &[
        "errors",
        "valid",
        "max-errors",
        "format",
        "delimiter",
        "registry",
        "output",
        "config",
        "top-errors",
        "rows",
    ];

    pub(crate) fn parse(args: &[String]) -> Result<Args, String> {
        let mut positional = Vec::new();
        let mut flags = std::collections::HashMap::new();
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if let Some(rest) = arg.strip_prefix("--") {
                if let Some((key, value)) = rest.split_once('=') {
                    flags.insert(key.to_string(), value.to_string());
                } else if Self::VALUE_FLAGS.contains(&rest)
                    && i + 1 < args.len()
                    && !args[i + 1].starts_with("--")
                {
                    flags.insert(rest.to_string(), args[i + 1].clone());
                    i += 1;
                } else {
                    flags.insert(rest.to_string(), String::new());
                }
            } else {
                positional.push(arg.clone());
            }
            i += 1;
        }
        Ok(Args { positional, flags })
    }

    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.flags.get(key).map(String::as_str)
    }

    pub(crate) fn has(&self, key: &str) -> bool {
        self.flags.contains_key(key)
    }
}

/// Parse a single-character CSV delimiter flag value.
pub(crate) fn check_delimiter(value: Option<&str>) -> Result<u8, String> {
    match value {
        None => Ok(b','),
        Some(d) => {
            let bytes = d.as_bytes();
            if bytes.len() != 1 {
                return Err("--delimiter must be a single character".into());
            }
            Ok(bytes[0])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags_and_positionals() {
        let args: Vec<String> = ["check", "s.json", "--format=json", "--fail-fast", "d.json"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let parsed = Args::parse(&args).unwrap();
        assert_eq!(parsed.positional, vec!["check", "s.json", "d.json"]);
        assert_eq!(parsed.get("format"), Some("json"));
        assert!(parsed.has("fail-fast"));
    }
}
