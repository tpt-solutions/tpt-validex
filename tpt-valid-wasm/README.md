# tpt-valid-wasm

[![npm](https://img.shields.io/npm/v/tpt-validex.svg)](https://www.npmjs.com/package/tpt-validex)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**WebAssembly bindings for tpt-validex, built with wasm-bindgen.**

This crate builds the WASM module published to npm as `tpt-validex`. It runs
client-side validation in browsers, edge workers (Cloudflare Workers, Deno
Deploy, Fastly), and Node.js ≥ 18 — a 1.36 MB module with the same engine, the
same JSON Schema, and the same error format as every other binding.

The JS ⇄ `serde_json::Value` conversion is hand-rolled on top of `js-sys` so
that schemas and results are **plain JS objects** — destructuring
`const { isValid, errors } = validator.validate(data)` works exactly as
advertised, with no wrapper objects or class instances.

## Install

```sh
npm install tpt-validex
```

## Usage

```js
import init, { Validator, version } from 'tpt-validex';
await init();

const validator = new Validator({
  type: 'object',
  properties: {
    name: { type: 'string', minLength: 1 },
    age:  { type: 'integer', minimum: 0, maximum: 150 },
  },
  required: ['name', 'age'],
});

const { isValid, errors } = validator.validate({ name: 'Alice', age: 30 });
// isValid === true, errors === []

const bad = validator.validate({ name: '', age: 200 });
// bad.errors:
// [
//   { path: '$.name', message: '...', expected: 'minLength 1', actual: '0', value: '' },
//   { path: '$.age',  message: '...', expected: 'maximum 150', actual: '200', value: 200 },
// ]
```

Compile from a JSON string with `Validator.fromJson(schemaText)`. Invalid
schemas throw an `Error` at construction time. Validation itself is synchronous.

## API

| Member | Returns | Notes |
| :--- | :--- | :--- |
| `new Validator(schema)` | `Validator` | From a plain object or a boolean schema. |
| `Validator.fromJson(text)` | `Validator` | From schema JSON text. |
| `validate(data)` | `{ isValid, errors }` | Plain object out; collects all errors. |
| `isValid(data)` | `boolean` | Fail-fast, no error materialization. |
| `validateBatch(batch)` | `{ index, isValid, errors }[]` | One entry per input item, in order. |
| `warnings()` | `string[]` | Non-fatal schema-compile warnings. |
| `version()` | `string` | Library version, e.g. `'0.1.0'`. |
| `init(input?)` | `Promise` | The wasm-bindgen loader; `await init()` with no arguments under a bundler. |

Inputs are plain JS values. Objects and arrays are converted structurally;
`null`/`undefined`, booleans, numbers, and strings map to their JSON
equivalents. Functions, symbols, and class instances are **rejected** with a
descriptive error, since they have no JSON representation.

TypeScript declarations are shipped as `tpt-validex.d.ts` (`ValidationError`,
`ValidateResult`, `BatchResult`, `JsonSchema`, `ValidationStats`, `Validator`,
`version`, `init`).

## Browser form validation

A realistic use case — validating a form before submit, with the errors mapped
onto the fields:

```js
const validator = new Validator({
  type: 'object',
  properties: {
    email: { type: 'string', format: 'email' },
    age:   { type: 'integer', minimum: 0, maximum: 150 },
  },
  required: ['email', 'age'],
});

form.addEventListener('submit', (e) => {
  e.preventDefault();
  const { isValid, errors } = validator.validate(Object.fromEntries(new FormData(form)));
  if (!isValid) {
    for (const err of errors) {
      // err.path is a JSON path like "$.email"
      showFieldError(err.path.replace('$.', ''), err.message);
    }
  }
});
```

## Development

```sh
cd tpt-valid-wasm

# Web target (browsers, edge workers)
npm run build        # cargo build --target wasm32-unknown-unknown + wasm-bindgen --target web

# Node target
npm run build:node   # wasm-bindgen --target nodejs

npm test             # node --test tests/validator.test.mjs  (8 tests)
```

The `wasm-bindgen` version is pinned to `=0.2.129` in the workspace manifest so
the generated glue in `pkg/` always matches the installed `wasm-bindgen-cli`.

| Path | Purpose |
| :--- | :--- |
| `src/lib.rs` | The wasm-bindgen bindings and the JS ⇄ JSON conversion. |
| `tpt-validex.d.ts` | Ergonomic public TypeScript types. |
| `package.json` | npm metadata, build/test scripts, published `files`. |
| `pkg/` | Generated wasm-bindgen glue (build output). |
| `tests/validator.test.mjs` | Node test suite. |

## Performance

The WASM module is 1.36 MB (pre-`wasm-opt`), under the 5 MB budget, with a CI
size check. On the JS boundary this binding reaches ~514k objects/s; the
JS ⇄ wasm conversion cost dominates, and in-process engines like `zod` are
~2.7× faster on raw throughput. Choose tpt-validex for WASM when you need one
schema definition across languages, JSON Schema compatibility, or
server/browser-identical validation. Full numbers in
[../docs/performance.md](../docs/performance.md).

## Related crates

| Crate | Relationship |
| :--- | :--- |
| [`tpt-valid-core`](../tpt-valid-core) | The engine behind these bindings. |
| [`tpt-valid-schema`](../tpt-valid-schema) | Schema compilation used by `Validator`. |

## Documentation

- [JavaScript / TypeScript API reference](../docs/api-javascript.md)
- [Compliance matrix](../docs/compliance.md)
- [Performance](../docs/performance.md)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option.
