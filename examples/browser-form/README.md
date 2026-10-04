# Browser form validation with tpt-validex (WASM)

A single static page: the schema is compiled once in the browser, each
keystroke validates the form state client-side. No backend involved.

## Run

```sh
wasm-pack build tpt-valid-wasm --target web --out-dir pkg   # repo root
python -m http.server 8000        # from this folder
# open http://localhost:8000
```
