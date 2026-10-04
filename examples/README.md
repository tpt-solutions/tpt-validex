# Examples

Runnable, self-contained projects showing `tpt-validex` in real settings.
Each folder has its own README with run instructions.

| Example | Stack | What it shows |
| :--- | :--- | :--- |
| [`fastapi/`](fastapi/) | Python + FastAPI | request-body validation with typed errors |
| [`express-middleware/`](express-middleware/) | Node.js + WASM | Express middleware validating JSON payloads |
| [`pandas-etl/`](pandas-etl/) | Python + pandas | streaming CSV validation inside an ETL step |
| [`go-http-handler/`](go-http-handler/) | Go (cgo) | HTTP handler validating request bodies |
| [`browser-form/`](browser-form/) | Browser + WASM | client-side form validation, zero backend |
| [`streamforge-pipeline/`](streamforge-pipeline/) | Rust | `tpt-streamforge` pipeline with a validate stage |

For the CLI equivalents see `docs/cli.md` (`validex check`, `validex infer`).
