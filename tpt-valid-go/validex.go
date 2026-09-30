// Package validex provides Go bindings for tpt-validex, the universal
// high-performance data validation engine.
//
// One schema, every language: validate with the same JSON Schema (Draft
// 2020-12 subset) in Go, Python, JavaScript, C/C++, and Rust — backed by
// the same Rust core compiled to a native library.
//
// # Quick start
//
//	validator, err := validex.NewValidator(`{
//	    "type": "object",
//	    "properties": {"age": {"type": "integer", "minimum": 0}},
//	    "required": ["age"]
//	}`)
//	if err != nil {
//	    panic(err)
//	}
//	defer validator.Close()
//
//	isValid, errors := validator.Validate(map[string]interface{}{"age": 30})
//
// # Building
//
// This package uses cgo and requires the native tpt-validex library:
//
//	cargo build --release -p tpt-valid-ffi
//
// then point the compiler at the headers/library (see README.md), e.g.:
//
//	CGO_CFLAGS="-I<path>/tpt-valid-ffi" \
//	CGO_LDFLAGS="-L<path>/target/release" go test ./...
//
// tpt-validex is dual-licensed under MIT and Apache-2.0.
package validex

// #cgo LDFLAGS: -ltpt_valid_ffi
// #include <stdlib.h>
// #include "tpt_validex.h"
//
// // Bridge to the //export trampoline below; needed so Go code can take
// // its address as a tpt_valid_format_cb.
// int validex_format_trampoline(void* userData, const char* value);
import "C"

import (
	"encoding/json"
	"errors"
	"fmt"
	"sync"
	"unsafe"
)

// ValidationError is one structured validation error (mirrors the spec §5.5
// error report).
type ValidationError struct {
	Path     string `json:"path"`
	Message  string `json:"message"`
	Expected string `json:"expected"`
	Actual   string `json:"actual"`
}

func (e ValidationError) Error() string {
	return fmt.Sprintf("%s: %s", e.Path, e.Message)
}

// Result is the outcome of one item in ValidateBatch.
type Result struct {
	Index   int
	IsValid bool
	Errors  []ValidationError
}

// Validator is a compiled JSON Schema validator.
//
// A Validator is safe for concurrent use by multiple goroutines: the
// underlying engine is thread-safe.
type Validator struct {
	handle *C.tpt_valid_handle
}

// NewValidator compiles a JSON Schema (Draft 2020-12 subset).
func NewValidator(schema string) (*Validator, error) {
	cSchema := C.CString(schema)
	defer C.free(unsafe.Pointer(cSchema))

	handle := C.tpt_valid_create(cSchema)
	if handle == nil {
		return nil, errors.New("tpt-validex: invalid schema: " + C.GoString(C.tpt_valid_last_error()))
	}
	return &Validator{handle: handle}, nil
}

// Close destroys the validator and releases native resources.
// Calling Close twice is safe; Validate after Close returns an error.
func (v *Validator) Close() error {
	if v == nil || v.handle == nil {
		return nil
	}
	C.tpt_valid_destroy(v.handle)
	v.handle = nil
	return nil
}

func (v *Validator) ensureOpen() error {
	if v == nil || v.handle == nil {
		return errors.New("tpt-validex: validator is closed")
	}
	return nil
}

// validateJSON validates one raw JSON document, returning validity and the
// parsed error list.
func (v *Validator) validateJSON(data string) (bool, []ValidationError, error) {
	if err := v.ensureOpen(); err != nil {
		return false, nil, err
	}
	cData := C.CString(data)
	defer C.free(unsafe.Pointer(cData))

	result := C.tpt_valid_validate(v.handle, cData)
	if result == nil {
		return false, nil, errors.New("tpt-validex: validation failed: " + C.GoString(C.tpt_valid_last_error()))
	}
	defer C.tpt_valid_free_result(result)

	valid := bool(C.tpt_valid_is_valid(result))
	errs, err := parseErrors(C.GoString(C.tpt_valid_get_errors(result)))
	if err != nil {
		return valid, nil, err
	}
	return valid, errs, nil
}

// Validate checks one document against the schema.
//
// The document is serialized with encoding/json, so any JSON-marshalable
// value works (maps, structs, slices, scalars).
func (v *Validator) Validate(data interface{}) (bool, []ValidationError, error) {
	raw, err := json.Marshal(data)
	if err != nil {
		return false, nil, fmt.Errorf("tpt-validex: cannot serialize data: %w", err)
	}
	return v.validateJSON(string(raw))
}

// ValidateBatch validates a batch of documents in parallel using goroutines.
// The returned results keep input order.
func (v *Validator) ValidateBatch(batch []interface{}) ([]Result, error) {
	results := make([]Result, len(batch))

	// Encode on this goroutine (encoding/json is not always cheap), then
	// validate concurrently — the native engine is thread-safe.
	type job struct {
		index int
		raw   string
		err   error
	}
	jobs := make([]job, len(batch))
	for i, item := range batch {
		raw, err := json.Marshal(item)
		jobs[i] = job{index: i, raw: string(raw), err: err}
	}

	sem := make(chan struct{}, 8)
	var wg sync.WaitGroup
	var firstErr error
	var mu sync.Mutex

	for _, j := range jobs {
		wg.Add(1)
		go func(j job) {
			defer wg.Done()
			sem <- struct{}{}
			defer func() { <-sem }()

			if j.err != nil {
				mu.Lock()
				if firstErr == nil {
					firstErr = fmt.Errorf("tpt-validex: cannot serialize item %d: %w", j.index, j.err)
				}
				mu.Unlock()
				return
			}
			valid, errs, err := v.validateJSON(j.raw)
			if err != nil {
				mu.Lock()
				if firstErr == nil {
					firstErr = err
				}
				mu.Unlock()
				return
			}
			results[j.index] = Result{Index: j.index, IsValid: valid, Errors: errs}
		}(j)
	}
	wg.Wait()

	if firstErr != nil {
		return nil, firstErr
	}
	return results, nil
}

// Version returns the native library version string.
func Version() string {
	return C.GoString(C.tpt_valid_version())
}

// formatRegistry maps opaque ids to Go format functions. The id is passed
// through the C callback's user_data pointer.
var formatRegistry = struct {
	sync.Mutex
	next uintptr
	fns  map[uintptr]func(string) bool
}{fns: make(map[uintptr]func(string) bool)}

//export validex_format_trampoline
func validex_format_trampoline(userData unsafe.Pointer, value *C.char) C.int {
	formatRegistry.Lock()
	fn, ok := formatRegistry.fns[uintptr(userData)]
	formatRegistry.Unlock()
	if !ok {
		return 0
	}
	if fn(C.GoString(value)) {
		return 1
	}
	return 0
}

// RegisterFormat registers a custom format assertion on this validator.
//
// From then on, strings in fields whose schema carries "format": name
// (and which is not a built-in format) are validated by calling fn.
// Pass nil as fn to unregister a name. Unregistered custom formats are
// ignored, matching JSON Schema annotation semantics. RegisterFormat must
// not be called concurrently with Validate.
func (v *Validator) RegisterFormat(name string, fn func(string) bool) error {
	if err := v.ensureOpen(); err != nil {
		return err
	}
	var id uintptr
	if fn != nil {
		formatRegistry.Lock()
		formatRegistry.next++
		id = formatRegistry.next
		formatRegistry.fns[id] = fn
		formatRegistry.Unlock()
	}

	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	var cb C.tpt_valid_format_cb
	if fn != nil {
		cb = (C.tpt_valid_format_cb)(C.validex_format_trampoline)
	}
	rc := C.tpt_valid_register_format(v.handle, cName, cb, unsafe.Pointer(id))
	if rc != 0 {
		if fn != nil {
			formatRegistry.Lock()
			delete(formatRegistry.fns, id)
			formatRegistry.Unlock()
		}
		return errors.New("tpt-validex: RegisterFormat failed: " + C.GoString(C.tpt_valid_last_error()))
	}
	return nil
}

// parseErrors decodes the FFI error report: {"errors": [...]}.
func parseErrors(report string) ([]ValidationError, error) {
	var parsed struct {
		Errors []ValidationError `json:"errors"`
	}
	if err := json.Unmarshal([]byte(report), &parsed); err != nil {
		return nil, fmt.Errorf("tpt-validex: malformed error report: %w", err)
	}
	return parsed.Errors, nil
}
