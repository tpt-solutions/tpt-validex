#!/usr/bin/env bash
# Validate a set of files against one schema, merging SARIF results.
# Used by the repository's composite GitHub Action (action.yml).
set -uo pipefail

schema="$1"; shift
fail_on_invalid="$1"; shift
exit_code=0
merged="validex-results.sarif"

first=1
for f in "$@"; do
  echo "validating $f"
  if ! validex check "$schema" "$f" --format sarif > "$f.sarif.tmp"; then
    exit_code=1
  fi
  if [ $first -eq 1 ]; then
    cp "$f.sarif.tmp" "$merged.part"
    first=0
  else
    # Naive merge: concat results arrays via python if present, else keep last.
    if command -v python >/dev/null; then
      SARIF_A="$merged.part" SARIF_B="$f.sarif.tmp" OUT="$merged.part" python - <<'PY'
import json, os
a = json.load(open(os.environ["SARIF_A"]))
b = json.load(open(os.environ["SARIF_B"]))
a["runs"][0]["results"] += b["runs"][0].get("results", [])
json.dump(a, open(os.environ["OUT"], "w"))
PY
    else
      cp "$f.sarif.tmp" "$merged.part"
    fi
  fi
  rm -f "$f.sarif.tmp"
done
[ -f "$merged.part" ] && mv "$merged.part" "$merged"

if [ "$exit_code" -ne 0 ] && [ "$fail_on_invalid" = "true" ]; then
  exit 1
fi
exit 0
