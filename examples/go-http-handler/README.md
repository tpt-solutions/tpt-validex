# Go HTTP handler with tpt-validex

Validates JSON request bodies through the cgo bindings, returning structured
errors to clients.

## Run

```sh
cargo build --release -p tpt-valid-ffi              # repo root
CGO_CFLAGS="-I../../tpt-valid-ffi" \
CGO_LDFLAGS="-L../../target/release" \
go run handler.go
```

```sh
curl -s localhost:8080/users -H 'content-type: application/json' \
     -d '{"name": "", "age": -1}'
```
