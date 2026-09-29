"""Python tests for tpt-validex (run with pytest after `maturin develop`)."""

import os
import tempfile

import pytest

from tpt_validex import Validator

USER_SCHEMA = {
    "type": "object",
    "properties": {
        "name": {"type": "string", "minLength": 1},
        "age": {"type": "integer", "minimum": 0, "maximum": 150},
        "email": {"type": "string", "format": "email"},
    },
    "required": ["name", "age"],
}


@pytest.fixture(scope="module")
def validator() -> Validator:
    return Validator(USER_SCHEMA)


def test_valid_object(validator):
    is_valid, errors = validator.validate({"name": "Alice", "age": 30})
    assert is_valid
    assert errors == []


def test_invalid_object_collects_all_errors(validator):
    is_valid, errors = validator.validate({"name": "", "age": 200})
    assert not is_valid
    paths = sorted(e["path"] for e in errors)
    assert paths == ["$.age", "$.name"]
    age_error = next(e for e in errors if e["path"] == "$.age")
    assert age_error["expected"].startswith("maximum")
    assert age_error["actual"] == "200"


def test_type_error_shape(validator):
    is_valid, errors = validator.validate({"name": "Bob", "age": "25"})
    assert not is_valid
    assert errors[0]["path"] == "$.age"
    assert errors[0]["expected"] == "integer"
    assert errors[0]["actual"] == "string"
    assert errors[0]["value"] == "25"


def test_email_format(validator):
    is_valid, _ = validator.validate({"name": "A", "age": 1, "email": "a@b.com"})
    assert is_valid
    is_valid, errors = validator.validate({"name": "A", "age": 1, "email": "nope"})
    assert not is_valid
    assert errors[0]["path"] == "$.email"


def test_schema_from_json_string():
    validator = Validator.from_json('{"type": "integer"}')
    assert validator.is_valid(42)
    assert not validator.is_valid("42")


def test_boolean_schema():
    validator = Validator(True)
    assert validator.is_valid({"anything": 1})


def test_invalid_schema_raises():
    with pytest.raises(ValueError):
        Validator({"minimum": 10, "maximum": 5})


def test_is_valid_short_circuit(validator):
    assert validator.is_valid({"name": "A", "age": 1})
    assert not validator.is_valid({})


def test_batch(validator):
    batch = [
        {"name": "Alice", "age": 30},
        {"name": "Bob", "age": "25"},
        {"name": "", "age": 200},
    ]
    results = validator.validate_batch(batch)
    assert [r.index for r in results] == [0, 1, 2]
    assert [r.is_valid for r in results] == [True, False, False]
    errors = results[2].errors()
    assert len(errors) == 2


def test_empty_batch(validator):
    assert validator.validate_batch([]) == []


def test_unsupported_input_type(validator):
    with pytest.raises(ValueError):
        validator.validate(object())


def test_csv_streaming(validator, tmp_path):
    input_csv = tmp_path / "input.csv"
    input_csv.write_text(
        "name,age,email\n"
        "Alice,30,alice@example.com\n"
        "Bob,25,bob example.com\n"
        "Carol,40,carol@example.com\n",
        encoding="utf-8",
    )
    valid_csv = tmp_path / "valid.csv"
    errors_jsonl = tmp_path / "errors.jsonl"

    stats = validator.validate_csv(str(input_csv), str(valid_csv), str(errors_jsonl))
    assert stats == {"total_rows": 3, "valid_rows": 2, "invalid_rows": 1, "parse_errors": 0}

    valid_lines = valid_csv.read_text(encoding="utf-8").splitlines()
    assert valid_lines == [
        "Alice,30,alice@example.com",
        "Carol,40,carol@example.com",
    ]
    errors = errors_jsonl.read_text(encoding="utf-8").splitlines()
    assert len(errors) == 1
    assert '"line":3' in errors[0]
    assert '"path":"$.email"' in errors[0]


def test_csv_stats_only(validator, tmp_path):
    input_csv = tmp_path / "input.csv"
    input_csv.write_text("age\n1\n-1\n", encoding="utf-8")
    schema_only = Validator(
        {
            "type": "object",
            "properties": {"age": {"type": "integer", "minimum": 0}},
        }
    )
    stats = schema_only.validate_csv(str(input_csv))
    assert stats["total_rows"] == 2
    assert stats["valid_rows"] == 1


def test_csv_custom_delimiter(tmp_path):
    validator = Validator({"type": "object"})
    input_csv = tmp_path / "input.csv"
    input_csv.write_text("a;b\n1;2\n", encoding="utf-8")
    stats = validator.validate_csv(str(input_csv), delimiter=";")
    assert stats["total_rows"] == 1
    assert stats["valid_rows"] == 1


def test_jsonl_streaming(tmp_path):
    validator = Validator.from_json('{"type": "object", "required": ["age"]}')
    input_jsonl = tmp_path / "input.jsonl"
    input_jsonl.write_text('{"age": 1}\n{"other": 2}\nnot json\n', encoding="utf-8")
    errors_jsonl = tmp_path / "errors.jsonl"

    stats = validator.validate_jsonl(str(input_jsonl), str(errors_jsonl))
    assert stats == {"total_lines": 3, "valid_lines": 1, "invalid_lines": 1, "parse_errors": 1}
    errors = errors_jsonl.read_text(encoding="utf-8").splitlines()
    assert len(errors) == 2


def test_warnings_unknown_format():
    validator = Validator({"type": "string", "format": "color"})
    assert len(validator.warnings()) == 1


def test_version():
    from tpt_validex import __version__

    assert isinstance(__version__, str)
    assert __version__.count(".") >= 1
