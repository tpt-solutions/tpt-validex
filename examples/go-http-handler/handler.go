// HTTP handler validating request bodies with tpt-validex.
package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"

	validex "github.com/tpt-solutions/tpt-validex-go"
)

const userSchema = `{
	"type": "object",
	"properties": {
		"name": {"type": "string", "minLength": 1},
		"age":  {"type": "integer", "minimum": 0}
	},
	"required": ["name", "age"]
}`

type errorReport struct {
	Errors []validex.ValidationError `json:"errors"`
}

func main() {
	validator, err := validex.NewValidator(userSchema)
	if err != nil {
		fmt.Fprintln(os.Stderr, "schema:", err)
		os.Exit(1)
	}
	defer validator.Close()

	http.HandleFunc("/users", func(w http.ResponseWriter, r *http.Request) {
		body, err := io.ReadAll(r.Body)
		if err != nil {
			http.Error(w, "cannot read body", http.StatusBadRequest)
			return
		}
		var payload any
		if err := json.Unmarshal(body, &payload); err != nil {
			http.Error(w, "invalid JSON", http.StatusBadRequest)
			return
		}
		isValid, validationErrors, err := validator.Validate(payload)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		if !isValid {
			w.WriteHeader(http.StatusUnprocessableEntity)
			_ = json.NewEncoder(w).Encode(errorReport{Errors: validationErrors})
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"user": payload, "status": "created"})
	})

	fmt.Println("listening on :8080")
	if err := http.ListenAndServe(":8080", nil); err != nil {
		fmt.Fprintln(os.Stderr, errors.Unwrap(err))
		os.Exit(1)
	}
}
