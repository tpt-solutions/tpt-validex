# C example

End-to-end demonstration of the tpt-validex C ABI.

## Build the library

```sh
cargo build --release -p tpt-valid-ffi
```

## Compile the example

Linux / macOS:

```sh
cc -I . main.c ../../target/release/libtpt_valid_ffi.so -o example   # Linux
cc -I . main.c ../../target/release/libtpt_valid_ffi.dylib -o example # macOS
```

Windows (MinGW gcc can link the DLL directly):

```sh
gcc -I . main.c ../../target/release/tpt_valid_ffi.dll -o example.exe
```

Windows (MSVC):

```sh
cl /I . main.c /link ..\..\target\release\tpt_valid_ffi.dll.lib
```

## Run

`./example` — keep the shared library next to the binary or on the library path.
