"""ETL ingest step: stream-validate CSV, then load the clean rows."""

import pandas as pd

from tpt_validex import Validator

SCHEMA = {
    "type": "object",
    "properties": {
        "customer": {"type": "string", "minLength": 1},
        "amount": {"type": "number", "minimum": 0},
        "country": {"type": "string", "format": "country-code"},
    },
    "required": ["customer", "amount"],
}


def ingest(raw_path: str, valid_path: str, rejects_path: str) -> pd.DataFrame:
    validator = Validator(SCHEMA)
    stats = validator.validate_csv(
        raw_path, valid_output=valid_path, errors_output=rejects_path
    )
    print(
        f"ingest: {stats['valid_rows']}/{stats['total_rows']} rows accepted "
        f"({stats['invalid_rows']} rejected, see {rejects_path})"
    )
    return pd.read_csv(valid_path)


if __name__ == "__main__":
    with open("raw.csv", "w") as f:
        f.write("customer,amount,country\n")
        f.write("Acme GmbH,120.50,DE\n")
        f.write(",9.99,US\n")          # missing customer -> rejected
        f.write("Beta Inc,-4,FR\n")    # negative amount -> rejected

    df = ingest("raw.csv", "clean.csv", "rejects.jsonl")
    print(df)
