//! Integration tests: CSV/JSONL → validate → valid output + errors.jsonl
//! (spec §5.6, §13 Use Case 2).

use std::io::Read;
use std::path::PathBuf;

use tpt_stream_core::Pipeline;
use tpt_valid_schema::Validator;

use crate::{ErrorMode, PipelineExt, ValidateConfig};

const USER_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age": {"type": "integer", "minimum": 0, "maximum": 150}
    },
    "required": ["name", "age"]
}"#;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-validex-streamforge-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &std::path::Path, content: &str) -> PathBuf {
    std::fs::write(path, content).unwrap();
    path.to_path_buf()
}

fn read_string(path: &std::path::Path) -> String {
    let mut s = String::new();
    std::fs::File::open(path)
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

#[tokio::test]
async fn csv_pipeline_splits_valid_and_errors() {
    let dir = temp_dir("csv-split");
    let input = write(
        &dir.join("input.csv"),
        "name,age\nAlice,30\n,200\nBob,25\nCarol,-5\n",
    );

    let validator = Validator::new(USER_SCHEMA).unwrap();
    let mut pipeline = Pipeline::new();
    pipeline
        .read_csv(input.to_string_lossy().to_string())
        .validate_with(
            &validator,
            ValidateConfig::new()
                .on_invalid(ErrorMode::Skip)
                .errors_to(dir.join("errors.jsonl")),
        )
        .write_csv(dir.join("valid.csv").to_string_lossy().to_string());
    let stats = pipeline.execute().await.unwrap();
    assert_eq!(stats.rows, 4, "source rows counted before stages");

    let valid_csv = read_string(&dir.join("valid.csv"));
    assert!(valid_csv.contains("Alice"));
    assert!(valid_csv.contains("Bob"));
    assert!(
        !valid_csv.contains("Carol"),
        "invalid rows dropped: {valid_csv}"
    );
    assert_eq!(valid_csv.lines().count(), 3, "header + 2 valid rows");

    let errors = read_string(&dir.join("errors.jsonl"));
    let lines: Vec<&str> = errors.lines().collect();
    assert_eq!(lines.len(), 2, "two invalid rows quarantined: {errors}");
    assert!(
        lines[0].contains(r#""line":2"#),
        "row 2 is invalid: {}",
        lines[0]
    );
    assert!(lines[0].contains(r#""path":"$.name""#));
    assert!(
        lines[1].contains(r#""line":4"#),
        "row 4 is invalid: {}",
        lines[1]
    );
    assert!(lines[1].contains(r#""path":"$.age""#));
}

#[tokio::test]
async fn jsonl_pipeline_with_abort_mode() {
    let dir = temp_dir("jsonl-abort");
    let input = write(
        &dir.join("input.jsonl"),
        "{\"name\": \"Alice\", \"age\": 30}\n{\"name\": \"Bob\", \"age\": \"25\"}\n",
    );

    let validator = Validator::new(USER_SCHEMA).unwrap();
    let mut pipeline = Pipeline::new();
    pipeline
        .read_jsonl(input.to_string_lossy().to_string())
        .validate_with(
            &validator,
            ValidateConfig::new().on_invalid(ErrorMode::Abort),
        )
        .write_jsonl(dir.join("valid.jsonl").to_string_lossy().to_string());
    let result = pipeline.execute().await;

    let err = result.expect_err("abort mode must fail the pipeline");
    let message = err.to_string();
    assert!(message.contains("validation failed"), "{message}");
    assert!(
        message.contains("$.age"),
        "names the offending path: {message}"
    );
}

#[tokio::test]
async fn skip_mode_default_drops_silently() {
    let dir = temp_dir("skip-default");
    let input = write(&dir.join("input.csv"), "name,age\nAlice,30\n,200\nBob,25\n");

    let validator = Validator::new(USER_SCHEMA).unwrap();
    let mut pipeline = Pipeline::new();
    pipeline
        .read_csv(input.to_string_lossy().to_string())
        .validate(&validator) // PipelineExt default: ErrorMode::Skip, no errors file
        .write_csv(dir.join("valid.csv").to_string_lossy().to_string());
    pipeline.execute().await.unwrap();

    let valid_csv = read_string(&dir.join("valid.csv"));
    assert!(valid_csv.contains("Alice") && valid_csv.contains("Bob"));
    assert!(
        !dir.join("errors.jsonl").exists(),
        "no errors file configured"
    );
}

#[tokio::test]
async fn log_mode_still_writes_errors_file() {
    let dir = temp_dir("log-mode");
    let input = write(&dir.join("input.csv"), "name,age\nAlice,30\n,200\nBob,25\n");

    let validator = Validator::new(USER_SCHEMA).unwrap();
    let mut pipeline = Pipeline::new();
    pipeline
        .read_csv(input.to_string_lossy().to_string())
        .validate_with(
            &validator,
            ValidateConfig::new()
                .on_invalid(ErrorMode::Log)
                .errors_to(dir.join("errors.jsonl")),
        )
        .write_csv(dir.join("valid.csv").to_string_lossy().to_string());
    pipeline.execute().await.unwrap();

    let errors = read_string(&dir.join("errors.jsonl"));
    assert_eq!(errors.lines().count(), 1);
    let valid_csv = read_string(&dir.join("valid.csv"));
    assert!(valid_csv.contains("Alice") && valid_csv.contains("Bob"));
}

#[tokio::test]
async fn all_valid_passes_through_untouched() {
    let dir = temp_dir("all-valid");
    let input = write(&dir.join("input.csv"), "name,age\nAlice,30\nBob,25\n");

    let validator = Validator::new(USER_SCHEMA).unwrap();
    let mut pipeline = Pipeline::new();
    pipeline
        .read_csv(input.to_string_lossy().to_string())
        .validate(&validator)
        .write_csv(dir.join("valid.csv").to_string_lossy().to_string());
    pipeline.execute().await.unwrap();

    let valid_csv = read_string(&dir.join("valid.csv"));
    assert!(valid_csv.contains("Alice") && valid_csv.contains("Bob"));
}

#[tokio::test]
async fn streamforge_row_values_map_to_json() {
    let dir = temp_dir("value-mapping");
    // score stays float, joined is parsed as a date column by streamforge.
    let input = write(
        &dir.join("input.csv"),
        "name,score,joined\nAlice,9.5,2026-09-29\n",
    );

    let validator = Validator::new(
        r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "score": {"type": "number"},
            "joined": {"type": "string", "format": "date"}
        },
        "required": ["name", "score", "joined"]
    }"#,
    )
    .unwrap();
    let mut pipeline = Pipeline::new();
    pipeline
        .read_csv(input.to_string_lossy().to_string())
        .validate(&validator)
        .write_csv(dir.join("valid.csv").to_string_lossy().to_string());
    let stats = pipeline.execute().await.unwrap();
    assert_eq!(stats.rows, 1, "date column must render as YYYY-MM-DD");
}

#[tokio::test]
async fn dsl_built_validator_works_as_stage() {
    let dir = temp_dir("dsl-validator");
    let input = write(&dir.join("input.csv"), "age\n30\n-1\n");

    // The schema! DSL flows through the same pipeline; build it via the
    // schema crate AST here to keep this crate's deps minimal.
    let ast = tpt_valid_schema::ast::parse_schema(&serde_json::json!({
        "type": "object",
        "properties": {"age": {"type": "integer", "minimum": 0}},
        "required": ["age"]
    }))
    .unwrap();
    let validator = Validator::from_ast(&ast).unwrap();

    let mut pipeline = Pipeline::new();
    pipeline
        .read_csv(input.to_string_lossy().to_string())
        .validate_with(
            &validator,
            ValidateConfig::new().errors_to(dir.join("errors.jsonl")),
        )
        .write_csv(dir.join("valid.csv").to_string_lossy().to_string());
    pipeline.execute().await.unwrap();

    assert!(read_string(&dir.join("valid.csv")).contains("30"));
    assert!(read_string(&dir.join("errors.jsonl")).contains(r#""path":"$.age""#));
}
