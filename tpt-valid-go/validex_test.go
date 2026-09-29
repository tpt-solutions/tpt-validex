package validex

import (
	"strings"
	"sync"
	"testing"
)

const userSchema = `{
	"type": "object",
	"properties": {
		"name": {"type": "string", "minLength": 1},
		"age":  {"type": "integer", "minimum": 0, "maximum": 150},
		"email": {"type": "string", "format": "email"}
	},
	"required": ["name", "age"]
}`

func newTestValidator(t *testing.T) *Validator {
	t.Helper()
	v, err := NewValidator(userSchema)
	if err != nil {
		t.Fatalf("NewValidator failed: %v", err)
	}
	t.Cleanup(func() { _ = v.Close() })
	return v
}

func TestNewValidatorInvalidSchema(t *testing.T) {
	if _, err := NewValidator(`{"minimum": 10, "maximum": 5}`); err == nil {
		t.Fatal("expected error for semantically invalid schema")
	}
	if _, err := NewValidator(`{invalid json`); err == nil {
		t.Fatal("expected error for malformed schema")
	}
}

func TestValidateValid(t *testing.T) {
	v := newTestValidator(t)
	valid, errs, err := v.Validate(map[string]interface{}{
		"name": "Alice",
		"age":  30,
	})
	if err != nil {
		t.Fatalf("Validate failed: %v", err)
	}
	if !valid || len(errs) != 0 {
		t.Fatalf("expected valid, got %v %v", valid, errs)
	}
}

func TestValidateCollectsAllErrors(t *testing.T) {
	v := newTestValidator(t)
	valid, errs, err := v.Validate(map[string]interface{}{
		"name": "",
		"age":  200,
	})
	if err != nil {
		t.Fatalf("Validate failed: %v", err)
	}
	if valid {
		t.Fatal("expected invalid")
	}
	if len(errs) != 2 {
		t.Fatalf("expected 2 errors, got %d: %v", len(errs), errs)
	}
	paths := map[string]bool{}
	for _, e := range errs {
		paths[e.Path] = true
		if e.Message == "" || e.Expected == "" {
			t.Fatalf("error missing fields: %+v", e)
		}
	}
	if !paths["$.name"] || !paths["$.age"] {
		t.Fatalf("unexpected error paths: %v", paths)
	}
}

func TestValidateStruct(t *testing.T) {
	v := newTestValidator(t)

	type User struct {
		Name string `json:"name"`
		Age  int    `json:"age"`
	}
	valid, errs, err := v.Validate(User{Name: "Bob", Age: 25})
	if err != nil || !valid || len(errs) != 0 {
		t.Fatalf("struct should validate: %v %v %v", valid, errs, err)
	}

	valid, errs, err = v.Validate(User{Name: "Bob", Age: -1})
	if err != nil {
		t.Fatalf("Validate failed: %v", err)
	}
	if valid || len(errs) != 1 {
		t.Fatalf("expected 1 error, got %v %v", valid, errs)
	}
}

func TestValidateRawJSON(t *testing.T) {
	v := newTestValidator(t)
	valid, _, err := v.validateJSON(`{"name": "A", "age": 1}`)
	if err != nil || !valid {
		t.Fatalf("raw json should validate: %v %v", valid, err)
	}
	valid, _, err = v.validateJSON(`{not json`)
	if err != nil {
		t.Fatalf("malformed data should return an invalid result, not error: %v", err)
	}
	if valid {
		t.Fatal("malformed data must be invalid")
	}
}

func TestValidateBatch(t *testing.T) {
	v := newTestValidator(t)
	results, err := v.ValidateBatch([]interface{}{
		map[string]interface{}{"name": "Alice", "age": 30},
		map[string]interface{}{"name": "Bob", "age": "25"},
		map[string]interface{}{"name": "", "age": 200},
	})
	if err != nil {
		t.Fatalf("ValidateBatch failed: %v", err)
	}
	if len(results) != 3 {
		t.Fatalf("expected 3 results, got %d", len(results))
	}
	for i, r := range results {
		if r.Index != i {
			t.Fatalf("result %d has index %d", i, r.Index)
		}
	}
	if !results[0].IsValid || results[1].IsValid || results[2].IsValid {
		t.Fatalf("unexpected validity: %+v", results)
	}
	if len(results[2].Errors) != 2 {
		t.Fatalf("expected 2 errors on item 2, got %d", len(results[2].Errors))
	}
}

func TestConcurrentValidation(t *testing.T) {
	v := newTestValidator(t)
	var wg sync.WaitGroup
	for i := 0; i < 32; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			valid, errs, err := v.Validate(map[string]interface{}{"name": "A", "age": 1})
			if err != nil || !valid || len(errs) != 0 {
				t.Errorf("concurrent validate failed: %v %v %v", valid, errs, err)
			}
		}()
	}
	wg.Wait()
}

func TestUseAfterClose(t *testing.T) {
	v, err := NewValidator(`{"type": "integer"}`)
	if err != nil {
		t.Fatalf("NewValidator failed: %v", err)
	}
	if err := v.Close(); err != nil {
		t.Fatalf("Close failed: %v", err)
	}
	if err := v.Close(); err != nil {
		t.Fatalf("double Close must be safe: %v", err)
	}
	if _, _, err := v.Validate(map[string]interface{}{}); err == nil {
		t.Fatal("expected error validating after Close")
	}
}

func TestVersion(t *testing.T) {
	if !strings.Contains(Version(), ".") {
		t.Fatalf("unexpected version: %q", Version())
	}
}
