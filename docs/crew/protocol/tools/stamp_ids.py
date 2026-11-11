#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Recompute the content-address `id` of every example object (Crew Protocol v0, unpublished).

    python3 docs/crew/protocol/tools/stamp_ids.py          rewrite ids in schemas/examples
    python3 docs/crew/protocol/tools/stamp_ids.py --check  fail when any id is stale

The id is `sha256:` plus the SHA-256 of the canonical form of the object without
its `id` (SPEC.md, "Ids"). Canonical form: UTF-8 JSON with no insignificant
whitespace, keys sorted by UTF-16 code unit, integers only. Standard library only.
"""

import hashlib
import json
import sys
from pathlib import Path

EXAMPLES = Path(__file__).resolve().parent.parent / "schemas" / "examples"


def canonical(value):
    if isinstance(value, bool) or value is None:
        return json.dumps(value)
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        raise ValueError("floating-point numbers are not allowed in Crew objects")
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, list):
        return "[" + ",".join(canonical(v) for v in value) + "]"
    keys = sorted(value, key=lambda k: k.encode("utf-16-be"))
    return "{" + ",".join(json.dumps(k, ensure_ascii=False) + ":" + canonical(value[k]) for k in keys) + "}"


def object_id(obj):
    body = {k: v for k, v in obj.items() if k != "id"}
    return "sha256:" + hashlib.sha256(canonical(body).encode("utf-8")).hexdigest()


def main():
    check = "--check" in sys.argv
    stale = []
    for path in sorted(EXAMPLES.glob("*.json")):
        obj = json.loads(path.read_text(encoding="utf-8"))
        if "id" not in obj:
            continue
        try:
            want = object_id(obj)
        except ValueError:
            continue  # an invalid example that carries a float cannot be addressed
        if obj["id"] == want:
            continue
        stale.append(path.name)
        if not check:
            obj["id"] = want
            path.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    if stale:
        print(("stale: " if check else "stamped: ") + ", ".join(stale))
    return 1 if (check and stale) else 0


if __name__ == "__main__":
    sys.exit(main())
