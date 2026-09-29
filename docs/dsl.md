# The `schema!` DSL

The `validex::schema!` macro is an ergonomic, compile-time-checked way to
declare schemas in Rust. The DSL AST flows through the **same** AST → IR →
optimizer → state-machine pipeline as JSON Schema — equivalent schemas
compile to identical machines (this is asserted by a test).

## Syntax

```rust
use validex::schema;

let validator = schema! {
    object {
        required "name"  => string(min_length = 1, max_length = 100),
        required "age"   => integer(min = 0, max = 150),
        optional "email" => string(format = "email"),
        optional "tags"  => array(items = string(), min_items = 0, max_items = 10),
        optional "meta"  => object {
            optional "source" => string(enum = ["web", "api"]),
            optional "score"  => number(exclusive_min = 0.0, multiple_of = 0.5),
        },
    }
};
```

The macro expression evaluates to a compiled `validex::Validator`.

## Grammar

```
schema  := type_expr
type    := "object" "{" entry* "}"
         | "array" "(" opts? ")"
         | ("string" | "integer" | "number" | "boolean" | "null" | "any") "(" opts? ")"
entry   := ("required" | "optional") STRING "=>" type ("," )?
opt     := IDENT "=" (STRING | INT | FLOAT | true | false | [list] | type_expr)
```

* The root may be any type (`schema! { integer(min = 0) }` is fine).
* `any` matches every value; `null` matches only `null`.
* Properties not marked `required` are `optional` (JSON Schema default).

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

The DSL is parsed and semantically validated **during macro expansion** —
these fail the build, not runtime:

* Syntax errors (missing `=>`, bad type names, trailing tokens).
* Unknown options or options on types that don't take them.
* Duplicate options / duplicate properties.
* `min > max`, `min_length > max_length`, `min_items > max_items`.
* `multiple_of <= 0`.
* Unknown `format` values (JSON Schema would ignore them; the DSL is strict).

Regex *syntax* can't be validated at compile time (no regex engine in the
macro); an invalid `pattern` panics at first use with a clear message.

## Interoperability

Because the DSL produces the same AST type as JSON Schema, you can mix:

```rust
let ast = validex::ast::parse_schema(&serde_json::json!({
    "type": "object",
    "properties": {"extra": {"type": "boolean"}}
}))?;
```

…or use `Validator::from_node` to wrap hand-built
`tpt_valid_core::ValidationNode` state machines directly.
