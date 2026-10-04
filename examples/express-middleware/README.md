# Express middleware with tpt-validex (WASM)

Validates JSON request bodies with the same engine in-process — no network
hop, no separate validation service.

## Run

```sh
# from the repo root: build the wasm package
wasm-pack build tpt-valid-wasm --target web --out-dir pkg
npm install express
node server.js
```

```sh
curl -s localhost:3000/orders -H 'content-type: application/json' \
     -d '{"sku": "", "qty": -5}'
```
