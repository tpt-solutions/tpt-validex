//! Benchmark: streaming CSV / JSONL validation throughput.
//!
//! Input is served from an in-memory buffer (`Cursor`) so the numbers isolate
//! parsing + validation, not disk I/O. Targets (spec §7): CSV > 10M rows/sec.

use std::io::Cursor;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use tpt_valid_core::{CsvDialect, ValidationOptions};
use tpt_valid_schema::Validator;

fn user_csv_schema() -> Validator {
    Validator::new(
        r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string", "minLength": 1},
                "age": {"type": "integer", "minimum": 0, "maximum": 150},
                "score": {"type": "number"}
            },
            "required": ["name", "age"]
        }"#,
    )
    .unwrap()
}

fn build_csv(rows: usize) -> String {
    let mut out = String::with_capacity(rows * 32);
    out.push_str("name,age,score\n");
    for i in 0..rows {
        out.push_str(&format!(
            "user{i},{},{}\n",
            i % 150,
            (i % 1000) as f64 / 10.0
        ));
    }
    out
}

fn build_jsonl(rows: usize) -> String {
    let mut out = String::with_capacity(rows * 64);
    for i in 0..rows {
        out.push_str(&format!(
            "{{\"name\": \"user{i}\", \"age\": {}, \"score\": {}}}\n",
            i % 150,
            (i % 1000) as f64 / 10.0
        ));
    }
    out
}

fn bench_streaming(c: &mut Criterion) {
    const ROWS: usize = 100_000;
    let csv = build_csv(ROWS);
    let jsonl = build_jsonl(ROWS);
    let validator = user_csv_schema();

    let mut group = c.benchmark_group("streaming");
    group.throughput(Throughput::Elements(ROWS as u64));

    group.bench_function("csv_100k_rows", |b| {
        b.iter(|| {
            let stats = validator
                .validate_csv(
                    Cursor::new(csv.as_bytes()),
                    &CsvDialect::default(),
                    &ValidationOptions::default(),
                )
                .unwrap();
            std::hint::black_box(stats.0.valid_rows);
        })
    });

    group.bench_function("jsonl_100k_lines", |b| {
        b.iter(|| {
            let stats = validator
                .validate_jsonl(Cursor::new(jsonl.as_bytes()), &ValidationOptions::default())
                .unwrap();
            std::hint::black_box(stats.valid_lines);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_streaming);
criterion_main!(benches);
