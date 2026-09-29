#!/usr/bin/env python
"""Apache-2.0-only dependency audit for tpt-validex (spec §3.2).

The project is dual-licensed MIT / Apache-2.0 and *rejects Apache-2.0-only
dependencies*. Because `cargo deny`'s allow list must contain "Apache-2.0"
for dual-licensed (MIT OR Apache-2.0) crates to evaluate, the "no
Apache-2.0-only" rule is enforced here: every crate reachable from the
workspace through *runtime* (non-dev) dependencies whose license expression
contains Apache-2.0 must also contain MIT.

Reviewed, allowed exceptions (permissive but not MIT):
    * "Apache-2.0 WITH LLVM-exception" (e.g. pyo3-ffi, target-lexicon)

Dev-only dependencies (benchmarks, test harnesses) are excluded: they are
never distributed to consumers. `cargo deny check licenses` still audits
them for copyleft.

Usage (from the workspace root):
    python scripts/audit_apache_only.py
"""

import json
import subprocess
import sys

ALLOWED_EXPRESSIONS = {
    "Apache-2.0 WITH LLVM-exception",
}


def has_mit(expression: str) -> bool:
    """Whether an SPDX expression includes an MIT grant."""
    return "MIT" in expression.upper()


def main() -> int:
    try:
        raw = subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1"],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except (OSError, subprocess.CalledProcessError) as e:
        print(f"failed to run cargo metadata: {e}")
        return 2

    meta = json.loads(raw)
    packages = {pkg["id"]: pkg for pkg in meta.get("packages", [])}
    resolve = meta.get("resolve", {})
    if not resolve:
        print("cargo metadata returned no resolve graph")
        return 2

    # Walk the dependency graph following only normal (non-dev) edges.
    workspace_members = set(meta.get("workspace_members", []))
    reachable: set[str] = set()
    stack = list(workspace_members)
    while stack:
        node_id = stack.pop()
        if node_id in reachable:
            continue
        reachable.add(node_id)
        node = next((n for n in resolve.get("nodes", []) if n["id"] == node_id), None)
        if node is None:
            continue
        for dep in node.get("deps", []):
            is_runtime = any(kind.get("kind") is None for kind in dep.get("dep_kinds", []))
            if is_runtime and dep["pkg"] in packages:
                stack.append(dep["pkg"])

    violations = []
    no_license = []
    for pkg_id in reachable:
        pkg = packages[pkg_id]
        if pkg_id in workspace_members:
            continue
        name = pkg.get("name", "?")
        version = pkg.get("version", "?")
        license_expr = (pkg.get("license") or "").strip()
        if not license_expr:
            no_license.append((name, version))
            continue
        if license_expr in ALLOWED_EXPRESSIONS:
            continue
        if "APACHE-2.0" not in license_expr.upper():
            continue
        if has_mit(license_expr):
            continue
        violations.append((name, version, license_expr))

    if no_license:
        print("warning: runtime crates without a license field:")
        for name, version in no_license:
            print(f"  - {name} {version}")

    if violations:
        print("Apache-2.0-only dependencies found (spec §3.2 violation):")
        for name, version, expr in violations:
            print(f"  - {name} {version} ({expr})")
        print("\nThese crates require explicit review: provide a MIT-licensed")
        print("replacement or document an approved exception in")
        print("ALLOWED_EXPRESSIONS / deny.toml.")
        return 1

    print(f"audit_apache_only: OK — {len(reachable) - len(workspace_members)} runtime "
          "crates checked, no Apache-2.0-only dependencies.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
