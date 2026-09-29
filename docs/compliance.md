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
| `items` | 🟡 | single-schema form only; array (tuple) form → ❌ |
| `prefixItems` | ❌ | tuple validation not implemented |
| `minItems` / `maxItems` | ✅ | |
| `uniqueItems` | ✅ | numeric-equality aware (`1` ≡ `1.0`) |
| `contains` | ✅ | at least one item matches; `minContains`/`maxContains` not supported |
| `minContains` / `maxContains` | ⚪ | ignored |
| `minLength` / `maxLength` | ✅ | counted in characters, not bytes |
| `pattern` | ✅ | ECMA-flavored subset via the `regex` crate (Unicode-aware) |
| `minimum` / `maximum` | ✅ | |
| `exclusiveMinimum` / `exclusiveMaximum` | ✅ | Draft 2020-12 numeric form (draft-4 boolean form not supported) |
| `multipleOf` | 🟡 | exact integer arithmetic for integral operands; epsilon-tolerant float path (documented quirk: `35`/`1.4` accepted) |
| `enum` | ✅ | hash-set O(1) lookup; numeric normalization (`1` ≡ `1.0`) |
| `const` | ✅ | deep equality with numeric normalization |
| `format` | 🟡 | validated for: `email`, `uri` (`url`, `iri` aliases), `date`, `date-time`, `uuid`, `ipv4`, `ipv6`, `hostname` (`idn-hostname` alias). Unknown formats are ignored with a warning per Draft 2020-12 §7.2.3; the DSL macro rejects unknown formats at compile time |
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
| `$id` / `$anchor` / `$defs` / `definitions` | ⚪ | ignored; subschemas are still validated in place |
| `$ref` | ❌ | reference resolution not implemented — inline the referenced schema (helpful error) |
| `$dynamicRef` / `$dynamicAnchor` | ❌ | not implemented |
| `dependsRequired` | ❌ | not implemented |
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
(see `cargo test -p tpt-valid-core -p tpt-valid-schema`). The full official
JSON-Schema-Test-Suite is not vendored; the matrix above is the contract —
each row maps to named test functions.
