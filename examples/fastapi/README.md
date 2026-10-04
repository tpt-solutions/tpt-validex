# FastAPI request validation with tpt-validex

Validates request bodies against a JSON Schema *before* the handler runs and
returns spec §5.5 structured errors.

## Run

```sh
pip install fastapi uvicorn maturin
# from the repo root: build the extension module in-place
maturin develop --release -m tpt-valid-py/Cargo.toml
uvicorn app:app --reload      # from this folder
```

```sh
curl -s localhost:8000/users -H 'content-type: application/json' \
     -d '{"name": "", "age": -1}' | python -m json.tool
```
