//! Benchmark: schema compilation time (target: < 10 ms, spec §7).
//!
//! Compilation covers the full pipeline: tokenizer → JSON → AST → semantic
//! checks → IR → optimizer → state machine. The cache is bypassed (`Validator::new`)
//! so each iteration is a fresh compile.

use criterion::{criterion_group, criterion_main, Criterion};
use tpt_valid_schema::Validator;

const SMALL_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {"age": {"type": "integer", "minimum": 0}},
    "required": ["age"]
}"#;

const USER_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1, "maxLength": 100},
        "age": {"type": "integer", "minimum": 0, "maximum": 150},
        "email": {"type": "string", "format": "email"},
        "website": {"type": "string", "format": "uri"},
        "joined": {"type": "string", "format": "date"},
        "id": {"type": "string", "format": "uuid"},
        "role": {"type": "string", "enum": ["admin", "user", "guest"]},
        "score": {"type": "number", "multipleOf": 0.5},
        "nick": {"type": "string", "pattern": "^[a-zA-Z][a-zA-Z0-9_]{2,15}$"}
    },
    "required": ["name", "age", "id"]
}"#;

fn bench_schema_compile(c: &mut Criterion) {
    let mut group = c.benchmark_group("schema_compile");

    group.bench_function("small_schema", |b| {
        b.iter(|| {
            let v = Validator::new(SMALL_SCHEMA).unwrap();
            std::hint::black_box(&v);
        })
    });

    group.bench_function("user_schema_9_properties", |b| {
        b.iter(|| {
            let v = Validator::new(USER_SCHEMA).unwrap();
            std::hint::black_box(&v);
        })
    });

    // Cached path: repeat lookups should be a map hit, not a recompile.
    group.bench_function("cached_lookup", |b| {
        b.iter(|| {
            let v = Validator::cached(USER_SCHEMA).unwrap();
            std::hint::black_box(&v);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_schema_compile);
criterion_main!(benches);
