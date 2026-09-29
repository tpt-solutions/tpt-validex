#!/usr/bin/env python
"""Comparison benchmark: tpt-validex vs pydantic vs jsonschema (Python).

Same dataset and schema shape as the Rust criterion suite (benches/validation.rs):
one flat user object, 10k-object batch.

Setup:
    cd tpt-valid-py && .venv/Scripts/python -m maturin develop --release
    .venv/Scripts/python -m pip install pydantic jsonschema pytest
Run:
    cd tpt-valid-py && .venv/Scripts/python ../benches/compare/compare_python.py
"""

import json
import time

BATCH = 10_000

SCHEMA = {
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1, "maxLength": 100},
        "age": {"type": "integer", "minimum": 0, "maximum": 150},
        "email": {"type": "string", "format": "email"},
        "tags": {"type": "array", "items": {"type": "string"}, "maxItems": 10},
    },
    "required": ["name", "age"],
}


def make_batch():
    return [
        {"name": f"user{i}", "age": i % 150, "email": f"user{i}@example.com", "tags": ["a", "b"]}
        for i in range(BATCH)
    ]


def bench(fn, data, min_time=1.0):
    """Run fn over data until min_time seconds; return objects/sec."""
    start = time.perf_counter()
    n = 0
    while True:
        fn(data)
        n += len(data)
        elapsed = time.perf_counter() - start
        if elapsed >= min_time:
            return n / elapsed


def bench_single(fn, value, min_time=1.0):
    start = time.perf_counter()
    n = 0
    while True:
        fn(value)
        n += 1
        elapsed = time.perf_counter() - start
        if elapsed >= min_time:
            return n / elapsed


def main():
    batch = make_batch()
    single = batch[0]
    results = {}

    # tpt-validex ---------------------------------------------------------
    from tpt_validex import Validator

    validator = Validator(SCHEMA)
    results["tpt-validex single"] = bench_single(validator.is_valid, single)
    results["tpt-validex batch"] = bench(lambda b: validator.validate_batch(b), batch)

    # jsonschema (Draft 2020-12 validator) --------------------------------
    try:
        from jsonschema import Draft202012Validator

        jsv = Draft202012Validator(SCHEMA)
        results["jsonschema single"] = bench_single(lambda v: jsv.is_valid(v), single)
        results["jsonschema batch"] = bench(lambda b: [jsv.is_valid(x) for x in b], batch)
    except ImportError:
        print("jsonschema not installed; skipping")

    # pydantic (model built from the same schema via TypeAdapter) ----------
    try:
        from pydantic import TypeAdapter

        adapter = TypeAdapter(dict)
        # pydantic needs a model to be meaningfully comparable:
        from pydantic import BaseModel, ConfigDict, Field, create_model

        model = create_model(
            "User",
            name=(str, Field(..., min_length=1, max_length=100)),
            age=(int, Field(..., ge=0, le=150)),
            email=(str | None, Field(None, pattern="^[^@]+@[^@]+$")),
            tags=(list[str] | None, None),
        )

        def pydantic_one(value):
            try:
                model.model_validate(value)
            except Exception:
                pass

        def pydantic_batch(b):
            for x in b:
                pydantic_one(x)

        results["pydantic single"] = bench_single(pydantic_one, single)
        results["pydantic batch"] = bench(pydantic_batch, batch)
    except ImportError:
        print("pydantic not installed; skipping")

    print(f"\nDataset: flat user object, batch of {BATCH:,} (release build)")
    print(f"{'Benchmark':<28}{'objects/sec':>15}")
    print("-" * 43)
    for name, ops in results.items():
        print(f"{name:<28}{ops:>15,.0f}")


if __name__ == "__main__":
    main()
