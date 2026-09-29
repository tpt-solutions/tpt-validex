/* C example for the tpt-validex FFI. Build instructions: see README.md. */

#include <stdio.h>
#include "tpt_validex.h"

int main(void) {
    const char* schema =
        "{"
        "  \"type\": \"object\","
        "  \"properties\": {"
        "    \"name\": {\"type\": \"string\", \"minLength\": 1},"
        "    \"age\":  {\"type\": \"integer\", \"minimum\": 0, \"maximum\": 150}"
        "  },"
        "  \"required\": [\"name\", \"age\"]"
        "}";

    tpt_valid_handle* validator = tpt_valid_create(schema);
    if (validator == NULL) {
        fprintf(stderr, "schema error: %s\n", tpt_valid_last_error());
        return 1;
    }

    const char* documents[] = {
        "{\"name\": \"Alice\", \"age\": 30}",  /* valid   */
        "{\"name\": \"Bob\", \"age\": \"25\"}", /* invalid */
        "{\"age\": 200}"                       /* invalid */
    };

    for (int i = 0; i < 3; i++) {
        tpt_valid_result* result = tpt_valid_validate(validator, documents[i]);
        if (tpt_valid_is_valid(result)) {
            printf("valid:   %s\n", documents[i]);
        } else {
            printf("invalid: %s\n  errors: %s\n", documents[i], tpt_valid_get_errors(result));
        }
        tpt_valid_free_result(result);
    }

    tpt_valid_destroy(validator);
    printf("tpt-validex version %s\n", tpt_valid_version());
    return 0;
}
