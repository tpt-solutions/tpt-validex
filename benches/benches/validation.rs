//! Benchmark: single-object and batch validation throughput.
//!
//! Targets (spec §7): > 5M objects/sec single-object; batch scales with cores.
//!
//! The hot loop validates a pre-parsed `serde_json::Value` (schema
//! compilation and JSON parsing are excluded so the number isolates the
//! validation state machine).

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use serde_json::{json, Value};
use tpt_valid_schema::Validator;

fn user_schema_json() -> &'static str {
    r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string", "minLength": 1, "maxLength": 100},
            "age": {"type": "integer", "minimum": 0, "maximum": 150},
            "email": {"type": "string", "format": "email"},
            "tags": {"type": "array", "items": {"type": "string"}, "maxItems": 10}
        },
        "required": ["name", "age"]
    }"#
}

fn sample_object() -> Value {
    json!({
        "name": "Alice",
        "age": 30,
        "email": "alice@example.com",
        "tags": ["admin", "user"]
    })
}

fn bench_single_object(c: &mut Criterion) {
    let validator = Validator::new(user_schema_json()).unwrap();
    let value = sample_object();

    let mut group = c.benchmark_group("validate/single");
    group.throughput(Throughput::Elements(1));
    group.bench_function("user_schema", |b| {
        b.iter(|| {
            let report = validator.validate(&value);
            std::hint::black_box(report.is_valid());
        })
    });
    group.finish();
}

fn bench_batch(c: &mut Criterion) {
    let validator = Validator::new(user_schema_json()).unwrap();
    const BATCH: usize = 10_000;
    let batch: Vec<Value> = (0..BATCH)
        .map(|i| {
            let mut v = sample_object();
            v["age"] = json!(i % 150);
            v
        })
        .collect();

    let mut group = c.benchmark_group("validate/batch");
    group.throughput(Throughput::Elements(BATCH as u64));
    group.bench_function("10k_objects_parallel", |b| {
        b.iter_batched(
            || batch.clone(),
            |batch| {
                let outcomes = validator.validate_batch(&batch);
                std::hint::black_box(outcomes.len());
            },
            BatchSize::LargeInput,
        )
    });
    group.finish();
}

fn bench_fail_fast_vs_collect_all(c: &mut Criterion) {
    let validator = Validator::new(user_schema_json()).unwrap();
    let invalid = json!({"name": "", "age": 999, "email": "bad", "tags": [1, 2, 3]});

    let mut group = c.benchmark_group("validate/invalid_document");
    group.bench_function("collect_all_errors", |b| {
        b.iter(|| {
            let report = validator.validate(&invalid);
            std::hint::black_box(report.errors.len());
        })
    });
    group.bench_function("fail_fast", |b| {
        b.iter(|| {
            let report =
                validator.validate_with(&invalid, &tpt_valid_core::ValidationOptions::fail_fast());
            std::hint::black_box(report.errors.len());
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_single_object,
    bench_batch,
    bench_fail_fast_vs_collect_all
);
criterion_main!(benches);
