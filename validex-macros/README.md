# validex-macros

[![Crates.io](https://img.shields.io/crates/v/validex-macros.svg)](https://crates.io/crates/validex-macros)
[![Docs.rs](https://img.shields.io/docsrs/validex-macros.svg)](https://docs.rs/validex-macros)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../LICENSE-MIT)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../LICENSE-APACHE)

**The `schema!` proc macro: declare validation schemas at compile time.**

This crate provides the `schema!` declarative macro used by
[`validex`](../validex). The macro parses the DSL during expansion, rejects
semantic errors with `compile_error!`, and emits AST-construction code that
flows through the **same** IR → state-machine pipeline as JSON Schema.

**Use it through the `validex` crate** — this is an implementation crate, and
`validex::schema!` is the supported entry point:

```toml
[dependencies]
validex = "0.1"
```

```rust
use validex::{schema, json};

let v = schema! {
    object {
        required "name"  => string(min_length = 1, max_length = 100),
        required "age"   => integer(min = 0, max = 150),
        optional "email" => string(format = "email"),
        optional "tags"  => array(items = string(), min_items = 0, max_items = 10),
    }
};

assert!(v.validate(&json!({"name": "Bob", "age": 25, "tags": ["a"]})).is_valid());
assert!(!v.validate(&json!({"name": "", "age": 999})).is_valid());
```

## Grammar

```text
schema  := type_expr
type    := "object" "{" entry* "}"
         | "array" "(" opts? ")"
         | ("string" | "integer" | "number" | "boolean" | "null" | "any") "(" opts? ")"
entry   := ("required" | "optional") STRING "=>" type ("," )?
opt     := IDENT "=" (STRING | INT | FLOAT | true | false | [list] | type_expr)
```

The root may be any type — `schema! { integer(min = 0) }` is valid. `any`
matches every value; `null` matches only `null`. Properties not marked
`required` are `optional` (the JSON Schema default).

## Options per type

| Type | Options |
| :--- | :--- |
| `string` | `min_length`, `max_length`, `pattern` (regex), `format`, `enum = [literals]` |
| `integer`, `number` | `min`, `max`, `exclusive_min`, `exclusive_max`, `multiple_of`, `enum = [literals]` |
| `array` | `items = <type>`, `min_items`, `max_items`, `unique` |
| `boolean`, `null`, `any` | none |
| `object` | block entries only (nest `object { ... }`) |

Known `format` values: `email`, `uri`/`url`, `date`, `date-time`, `uuid`,
`ipv4`, `ipv6`, `hostname`.

## Compile-time checking

The DSL is parsed and semantically validated **during macro expansion**, so
these fail the build rather than the runtime:

- Syntax errors (missing `=>`, bad type names, trailing tokens).
- Unknown options, or options on types that don't take them.
- Duplicate options and duplicate properties.
- `min > max`, `min_length > max_length`, `min_items > max_items`.
- `multiple_of <= 0`.
- Unknown `format` values — where JSON Schema would silently ignore them, the
  DSL is strict.

Regex *syntax* is the one thing that cannot be checked at compile time (there is
no regex engine in the macro); an invalid `pattern` panics at first use with a
clear message.

## Interoperability

Because the DSL produces the same AST type as JSON Schema, the two can be
mixed — see [../docs/dsl.md](../docs/dsl.md). A test in the `validex` crate
asserts that equivalent JSON Schema and DSL declarations lower to
byte-identical state machines.

## Design notes

- **Zero dependencies** beyond `proc_macro`. The macro hand-parses
  `proc_macro::TokenStream` into its own small DSL AST (`DslType`, `DslValue`,
  `DslOption`), validates it, then emits the construction code.
- Generated code addresses items via the `::validex::` path, which is why
  `validex` self-references in `[dev-dependencies]` so its own tests and
  doctests can expand `schema!`.

## Testing

```sh
cargo test -p validex-macros
cargo clippy -p validex-macros --all-targets -- -D warnings
cargo fmt --check
```

Compile-fail behaviors (invalid DSL) are exercised through the `validex` crate's
test suite, since they surface as `compile_error!` during expansion.

## Related crates

| Crate | Relationship |
| :--- | :--- |
| [`validex`](../validex) | The facade that re-exports this macro — use `validex::schema!`. |
| [`tpt-valid-schema`](../tpt-valid-schema) | Receives the emitted AST and compiles it. |
| [`tpt-valid-core`](../tpt-valid-core) | Executes the resulting state machine. |

## Documentation

- [DSL reference](../docs/dsl.md)
- [Compliance matrix](../docs/compliance.md)
- [Changelog](CHANGELOG.md)

## License

Dual-licensed under [MIT](../LICENSE-MIT) or [Apache-2.0](../LICENSE-APACHE), at
your option. This crate has **zero external dependencies**.
