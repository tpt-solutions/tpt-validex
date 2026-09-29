"""Type stubs for the tpt-validex extension module."""

from typing import Any

__version__: str


class ValidationErrorDict(dict[str, Any]):
    """Keyed error report: ``path``, ``message``, ``expected``, ``actual``, ``value``.

    Access as a plain dict, e.g. ``errors[0]["path"]``.
    """


class ValidationResult:
    """Result of one item in a :meth:`Validator.validate_batch` call."""

    @property
    def index(self) -> int: ...
    @property
    def is_valid(self) -> bool: ...
    def errors(self) -> list[dict[str, Any]]: ...

    def __repr__(self) -> str: ...


class Validator:
    """A compiled validator for one JSON Schema (Draft 2020-12 subset).

    Compile once, validate millions of records.

    Example::

        from tpt_validex import Validator

        validator = Validator({
            "type": "object",
            "properties": {"age": {"type": "integer", "minimum": 0}},
            "required": ["age"],
        })
        is_valid, errors = validator.validate({"age": 30})
    """

    def __init__(self, schema: dict[str, Any] | bool | str) -> None: ...

    @staticmethod
    def from_json(schema: str) -> Validator: ...

    def validate(self, data: Any) -> tuple[bool, list[dict[str, Any]]]:
        """Validate a single JSON-like object; returns ``(is_valid, errors)``."""
        ...

    def is_valid(self, data: Any) -> bool:
        """Boolean-only validation (fail-fast, no error materialization)."""
        ...

    def validate_batch(self, batch: list[Any]) -> list[ValidationResult]:
        """Validate a list of objects in parallel across all cores."""
        ...

    def validate_csv(
        self,
        input_path: str,
        valid_output: str | None = None,
        errors_output: str | None = None,
        *,
        delimiter: str = ",",
        has_headers: bool = True,
    ) -> dict[str, int]:
        """Stream-validate a CSV file; returns stats.

        Invalid rows are written to ``errors_output`` as JSONL objects
        (``{"line", "row", "errors"}``). Column types are inferred
        (bool/integer/number/string; empty cells become null).
        """
        ...

    def validate_jsonl(self, input_path: str, errors_output: str | None = None) -> dict[str, int]:
        """Stream-validate newline-delimited JSON; returns stats."""
        ...

    def warnings(self) -> list[str]:
        """Warnings collected during schema compilation (e.g. unknown formats)."""
        ...
