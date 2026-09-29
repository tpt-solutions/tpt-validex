# JavaScript / TypeScript API reference (`tpt-validex` on npm)

Install (bundled WASM — works in browsers, edge workers, and Node ≥ 18):

```sh
npm install tpt-validex
```

The package ships a 1.36 MB wasm module plus ergonomic TypeScript types
(`tpt-validex.d.ts`).

## Initialize

```js
// Bundlers / browsers: instantiate with the wasm asset
import init, { Validator, version } from 'tpt-validex';
await init();

// Node (the package's node build self-initializes):
import { Validator } from 'tpt-validex';
```

## `new Validator(schema)`

Compile a JSON Schema (Draft 2020-12 subset — see
[compliance.md](compliance.md)) from a plain object or boolean schema:

```js
const validator = new Validator({
  type: 'object',
  properties: {
    name: { type: 'string', minLength: 1 },
    age: { type: 'integer', minimum: 0, maximum: 150 },
  },
  required: ['name', 'age'],
});

Validator.fromJson(schemaText); // from a JSON string
```

Throws (`Error`) when the schema is invalid. Instances are cheap to share;
validation is synchronous.

### `validate(data) -> { isValid, errors }`

```js
const { isValid, errors } = validator.validate({ name: 'Alice', age: 30 });
// isValid === true, errors === []

const bad = validator.validate({ name: '', age: 200 });
// bad.errors:
// [
//   { path: '$.name', message: '...', expected: 'minLength 1', actual: '0', value: '' },
//   { path: '$.age',  message: '...', expected: 'maximum 150', actual: '200', value: 200 },
// ]
```

Inputs are plain JS values; objects/arrays are converted structurally.
Class instances, functions, and symbols are rejected.

### `isValid(data) -> boolean`

Boolean-only validation (fail-fast traversal) — the fastest option when you
don't need error details.

### `validateBatch(batch: unknown[]) -> { index, isValid, errors }[]`

Batch mode. On native runtimes this is parallel (rayon); in WASM it loops
but still amortizes the boundary cost:

```js
const results = validator.validateBatch(records);
results.filter((r) => !r.isValid).forEach((r) => console.log(r.index, r.errors));
```

### `warnings() -> string[]`

Non-fatal schema-compilation warnings (e.g. unknown `format` keywords).

## Browser form validation (spec §13 Use Case 3)

```html
<script type="module">
  import init, { Validator } from './pkg/tpt_valid_wasm.js';
  await init();

  const validator = new Validator({
    type: 'object',
    properties: {
      email: { type: 'string', format: 'email' },
      password: { type: 'string', minLength: 8 },
    },
    required: ['email', 'password'],
  });

  document.getElementById('form').addEventListener('submit', (e) => {
    const data = {
      email: document.getElementById('email').value,
      password: document.getElementById('password').value,
    };
    const { isValid, errors } = validator.validate(data);
    if (!isValid) {
      e.preventDefault();
      renderErrors(errors); // errors[i].path like "$.email"
    }
  });
</script>
```

## Building the pkg/ directory

```sh
cargo build -p tpt-valid-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --out-dir pkg --target web \
    target/wasm32-unknown-unknown/release/tpt_valid_wasm.wasm
# for Node tests:
wasm-bindgen --out-dir pkg --target nodejs <same wasm>
```

JS tests: `cd tpt-valid-wasm && npm test`.
