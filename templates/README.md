# Schema templates

Copy-paste starting points, each with a schema, a `good.json` and a
`bad.json` sample (the bad sample violates the schema in several ways at
once). Validate any of them with the CLI:

```sh
validex check templates/contact/schema.json templates/contact/good.json   # exit 0
validex check templates/contact/schema.json templates/contact/bad.json    # exit 1, structured errors
```

| Template | Highlights |
| :--- | :--- |
| [`contact/`](contact/) | formats (`email`, `phone`, `date`), unique tag arrays |
| [`ecommerce-order/`](ecommerce-order/) | nested item schema, `pattern` ids, `currency` format |
| [`invoice/`](invoice/) | VAT pattern, line items, tax-rate bounds |
| [`geojson-point/`](geojson-point/) | `const`, `prefixItems` tuple coordinates with bounds |
| [`log-line/`](log-line/) | `enum` levels, `uuid` trace ids |
| [`iot-event/`](iot-event/) | `propertyNames` metric keys, `semver` firmware |

OpenAPI request bodies: wrap any of these as
`{"schema": <template>, "application/json": ...}` inside an `content` object —
the schemas are plain Draft 2020-12.
