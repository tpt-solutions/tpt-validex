# JSON Schema Draft 2020-12 Compliance Matrix

`tpt-validex` implements a **deliberate, documented subset** of JSON Schema
Draft 2020-12. This page lists every commonly used keyword, its status, and
notes. Unsupported keywords are either rejected with a clear error at compile
time (so typos and unsupported constructs fail loudly) or ignored per the
spec's extensibility rules — the table says which.

Status legend: ✅ supported · 🟡 partial · ❌ rejected with compile error ·
⚪ ignored (no-op, per Draft 2020-12 §6.5 extensibility)

## Validation keywords

| Keyword | Status | Notes |
| :--- | :--- | :--- |
| `type` | ✅ | single or array form; `integer` matches whole floats (`1.0`) per spec |
| `properties` | ✅ | |
| `required` | ✅ | all missing keys reported (not just the first) |
| `additionalProperties` | ✅ | `false` or sub-schema; exact unknown-key accounting incl. `allOf` merging |
| `patternProperties` | ✅ | regex applied to every matching key |
| `items` | ✅ | single-schema form; array (tuple) form mapped to `prefixItems` |
| `prefixItems` | ✅ | positional schemas; schema-form `items` constrains the remainder |
| `minItems` / `maxItems` | ✅ | integral floats accepted (`1.0` ≡ `1`) |
| `uniqueItems` | ✅ | numeric-equality aware (`1` ≡ `1.0`) |
| `contains` | ✅ | |
| `minContains` / `maxContains` | ✅ | default `minContains` is 1; `minContains > maxContains` compiles to an unsatisfiable check (per spec, not a schema error) |
| `minProperties` / `maxProperties` | ✅ | |
| `minLength` / `maxLength` | ✅ | counted in characters, not bytes |
| `pattern` | ✅ | ECMA-flavored subset via the `regex` crate (Unicode-aware) |
| `minimum` / `maximum` | ✅ | |
| `exclusiveMinimum` / `exclusiveMaximum` | ✅ | Draft 2020-12 numeric form (draft-4 boolean form not supported) |
| `multipleOf` | 🟡 | exact integer arithmetic for integral operands; epsilon-tolerant float path (documented quirk: `35`/`1.4` accepted) |
| `enum` | ✅ | hash-set O(1) lookup; numeric normalization (`1` ≡ `1.0`) |
| `const` | ✅ | deep equality with numeric normalization |
| `format` | 🟡 | built-in: `email`, `uri` (`url`/`iri`), `date`, `date-time`, `time`, `duration`, `uuid`, `ipv4`, `ipv6`, `hostname` (`idn-hostname`), `phone`, `currency` (ISO 4217), `iban` (mod-97 checksum), `country-code` (ISO 3166-1 alpha-2), `semver`, `regex`, `json-pointer`. Unknown formats compile to runtime custom-format checks: they are no-ops unless an assertion is registered (Rust `ValidationOptions::with_format`, Python `Validator(schema, formats={...})`, JS `validator.registerFormat(name, fn)`, Go `validator.RegisterFormat`, C `tpt_valid_register_format`) — matching Draft 2020-12 §7.2.3 annotation semantics |
| `dependentRequired` | ✅ | missing dependents reported per key |
| `dependentSchemas` | ✅ | schema applied to the whole object when the key is present |
| `propertyNames` | ✅ | every object key validated as a string |
| `unevaluatedProperties` | 🟡 | static accounting: sees `properties`/`patternProperties` of the same schema object (incl. `allOf` siblings after merging) and is vacuous when `additionalProperties` is present; annotations crossing sibling `anyOf`/`oneOf`/`if` branches are **not** tracked |
| `unevaluatedItems` | 🟡 | same static-accounting caveat; constrains items beyond `prefixItems` when no `items` applies |
| `allOf` | ✅ | conjunction; object keywords merge (see below) |
| `anyOf` | ✅ | |
| `oneOf` | ✅ | exact-one semantics, including vacuous-match behavior |
| `not` | ✅ | |
| `if` / `then` / `else` | ✅ | |

### `allOf` merging semantics

`allOf` members are folded into the parent schema. Two observable
consequences:

1. Type sets intersect: `allOf: [{type: [a, b]}, {type: [b, c]}]` ≡ `type: b`.
2. Object keywords merge before `additionalProperties` accounting, so
   `additionalProperties: false` in the parent does **not** reject keys
   declared in a sibling member — matching the spec's intended semantics
   (this is the interaction most naive validators get wrong).

## Applicator / meta keywords

| Keyword | Status | Notes |
| :--- | :--- | :--- |
| `$schema` | ⚪ | ignored (Draft 2020-12 assumed) |
| `$defs` / `definitions` | ✅ | containers, resolved through `$ref` JSON Pointers |
| `$ref` | 🟡 | local `#/...` pointers, `#anchor` fragments (`$anchor`, `#`-form `$id`), and cross-document refs via a user-supplied registry (`Validator::new_with` / `SchemaRegistry`). Refs are expanded inline, so genuinely recursive schemas are detected and rejected; `$id` base-URI resolution (relative/URN refs) is not implemented |
| `$dynamicRef` / `$dynamicAnchor` | ❌ | rejected with a helpful compile error |
| `$anchor` | ✅ | as a `#name` ref target |
| `title`, `description`, `default`, `examples`, `deprecated`, `readOnly`, `writeOnly` | ⚪ | annotations, ignored |

## Formats (spec §5.4)

| Format | Implementation |
| :--- | :--- |
| `email` | hand-written practical RFC 5322 subset (dot-atom local part, DNS-style domain, length limits; single-label domains accepted) |
| `uri` | hand-written RFC 3986 absolute-URI check (`scheme:` + non-empty remainder, no controls/spaces) |
| `date` | ISO 8601 `YYYY-MM-DD` with calendar validation via `chrono` |
| `date-time` | RFC 3339 via `chrono` (lowercase `t`/`z` accepted, space separator rejected) |
| `uuid` | hand-written RFC 4122 textual form (any case; version/variant nibbles not enforced) |
| `ipv4` | hand-written; leading zeros rejected |
| `ipv6` | hand-written RFC 4291 incl. `::` compression and IPv4-mapped tails; zone IDs rejected |
| `hostname` | hand-written RFC 1123 subset |

## Error reporting

Errors follow spec §5.5: all violations are collected (fail-fast optional),
each carrying `path` (`$.address.zip`, `$.tags[2]`), `message`, `expected`,
`actual`, and `value`. The JSON envelope is `{"errors": [...]}`.

## Compliance test suite

`tpt-valid-schema` and `tpt-valid-core` unit tests cover every ✅/🟡 row above
(see `cargo test -p tpt-valid-core -p tpt-valid-schema`).

The **official JSON-Schema-Test-Suite (Draft 2020-12)** is vendored as a git
submodule (`third-party/JSON-Schema-Test-Suite`) and runs in CI
(`cargo test -p tpt-valid-schema --test jsonschema_suite`). Current status:
**884/884 non-skipped cases pass (100%)**; 417 cases are skipped through the
reviewed allow-list in `tpt-valid-schema/tests/jsonschema_suite.rs` — every
skip is annotated with its reason (annotation-tracking for `unevaluated*`,
`$id` base-URI resolution, dynamic refs, format-assertion vocabulary, and the
documented `allOf`/`additionalProperties` merge deviation). The suite run
prints the pass rate; update this page when it changes.
