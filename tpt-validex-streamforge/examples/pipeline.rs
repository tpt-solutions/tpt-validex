//! Streamforge pipeline with a validate stage (examples/streamforge-pipeline).
//!
//! Reads a CSV, validates every row against a compiled schema, writes the
//! clean rows to one file and rejects to a JSONL file.

use tpt_stream_core::Pipeline;
use tpt_valid_schema::Validator;
use tpt_validex_streamforge::{ErrorMode, PipelineExt, ValidateConfig};

#[tokio::main]
async fn main() -> Result<(), tpt_stream_core::Error> {
    let schema = Validator::new(
        r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string", "minLength": 1},
            "age": {"type": "integer", "minimum": 0}
        },
        "required": ["name", "age"]
    }"#,
    )
    .map_err(|e| tpt_stream_core::Error::Config(e.to_string()))?;

    let mut pipeline = Pipeline::new();
    pipeline
        .read_csv("input.csv")
        .validate_with(
            &schema,
            ValidateConfig::new()
                .on_invalid(ErrorMode::Skip)
                .errors_to("errors.jsonl"),
        )
        .write_csv("valid_output.csv");
    let stats = pipeline.execute().await?;
    println!("{stats:?}");
    Ok(())
}
