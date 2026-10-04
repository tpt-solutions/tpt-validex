#!/usr/bin/env python3
"""Memory profiling for tpt-validex streaming vs batch validation.

Generates a dataset of N records twice - once as JSONL (the CLI's streaming
path) and once as a single JSON array (the CLI's parallel batch path) - runs
`validex memcheck` over each, and reports the process peak RSS.

Expected result: streaming RSS is ~constant as N grows (O(1) per row);
batch RSS grows ~linearly with N (records are materialized).

Usage:
    python scripts/profile_memory.py [--rows 1000000] [--skip-batch]

Peak-RSS sources: `validex memcheck` reads the process high-water mark
directly (Windows GetProcessMemoryInfo, Linux VmHWM); on macOS the
script falls back to /usr/bin/time -l.
"""

from __future__ import annotations

import argparse
import json
import platform
import re
import subprocess
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CLI = REPO / "target" / "release" / ("validex.exe" if platform.system() == "Windows" else "validex")

SCHEMA = {
    "type": "object",
    "properties": {
        "id": {"type": "integer", "minimum": 0},
        "name": {"type": "string", "minLength": 1},
        "email": {"type": "string", "format": "email"},
        "score": {"type": "number", "minimum": 0, "maximum": 100},
    },
    "required": ["id", "name"],
}


def build_cli() -> None:
    if CLI.exists():
        return
    print("building release CLI (one-time)...")
    subprocess.run(
        ["cargo", "build", "--release", "-p", "tpt-valid-cli"],
        cwd=REPO,
        check=True,
    )


def generate(rows: int, tmp: Path) -> tuple[Path, Path]:
    """Write the JSONL (streaming input) and JSON array (batch input)."""
    jsonl = tmp / f"rows-{rows}.jsonl"
    batch = tmp / f"rows-{rows}.json"
    if jsonl.exists() and batch.exists():
        return jsonl, batch

    print(f"generating {rows:,} rows ...")
    with jsonl.open("w", encoding="utf-8") as f:
        for i in range(rows):
            record = {
                "id": i,
                "name": f"user-{i}",
                "email": f"user{i}@example.com",
                "score": (i % 1000) / 10.0,
            }
            f.write(json.dumps(record) + "\n")

    with jsonl.open("r", encoding="utf-8") as src, batch.open("w", encoding="utf-8") as dst:
        dst.write("[")
        first = True
        for row in src:
            if not first:
                dst.write(",")
            dst.write(row.strip())
            first = False
        dst.write("]")
    return jsonl, batch


def measure(mode_data: Path, schema_path: Path) -> tuple[int, int]:
    """Run `validex memcheck`; return (peak RSS bytes, failed records)."""
    if platform.system() == "Darwin" and Path("/usr/bin/time").exists():
        proc = subprocess.run(
            ["/usr/bin/time", "-l", str(CLI), "memcheck", str(schema_path), str(mode_data)],
            capture_output=True,
            text=True,
        )
        match = re.search(r"^\s*(\d+)\s+maximum resident set size", proc.stderr, re.M)
        peak = int(match.group(1)) * 1024 if match else 0
    else:
        proc = subprocess.run(
            [str(CLI), "memcheck", str(schema_path), str(mode_data)],
            capture_output=True,
            text=True,
        )
        peak = 0
    failed = 0
    m = re.search(r"failed_records (\d+)", proc.stdout)
    if m:
        failed = int(m.group(1))
    m = re.search(r"peak_rss_bytes (\d+)", proc.stdout)
    if m:
        peak = max(peak, int(m.group(1)))
    return peak, failed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rows", type=int, default=1_000_000)
    parser.add_argument("--skip-batch", action="store_true", help="skip the JSON-array run")
    args = parser.parse_args()

    build_cli()
    rows = args.rows
    with tempfile.TemporaryDirectory(prefix="validex-mem-") as tmp_raw:
        tmp = Path(tmp_raw)
        schema_path = tmp / "schema.json"
        schema_path.write_text(json.dumps(SCHEMA), encoding="utf-8")
        jsonl, batch = generate(rows, tmp)

        peak, failed = measure(jsonl, schema_path)
        print(f"streaming ({rows:,} JSONL rows): peak RSS = {peak / (1024 * 1024):.1f} MiB, {failed} failed")

        if not args.skip_batch:
            peak_batch, failed_batch = measure(batch, schema_path)
            print(
                f"batch ({rows:,}-element array): peak RSS = {peak_batch / (1024 * 1024):.1f} MiB, "
                f"{failed_batch} failed"
            )
            if peak > 0 and peak_batch > 0:
                print(f"batch/streaming ratio: {peak_batch / peak:.1f}x")

    print(
        "\nExpect streaming RSS to stay ~constant across --rows values; "
        "batch grows with the record count (records are materialized)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
