# pandas/Polars ETL step with streaming CSV validation

Validate an incoming CSV *before* it lands in the DataFrame; invalid rows go
to a reject file with line numbers, valid rows are loaded normally.

## Run

```sh
pip install pandas maturin
maturin develop --release -m tpt-valid-py/Cargo.toml   # repo root
python etl_step.py
```
