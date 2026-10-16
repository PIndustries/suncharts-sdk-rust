#!/usr/bin/env python3
"""Normalize the SunCharts OpenAPI 3.1 spec into the subset openapi-generator handles well, without changing
the wire contract: `type: [T, "null"]` -> `type: T` (+ nullable), `const: v` -> `enum: [v]`.
Usage: normalize-spec.py openapi.json > build/openapi.normalized.json"""
import json, sys

def fix(o):
    if isinstance(o, dict):
        t = o.get("type")
        if isinstance(t, list):
            rest = [x for x in t if x != "null"]
            if len(rest) == 1:
                o["type"] = rest[0]
                if "null" in t:
                    o["nullable"] = True
        if "const" in o and "enum" not in o:
            o["enum"] = [o.pop("const")]
        for v in o.values():
            fix(v)
    elif isinstance(o, list):
        for v in o:
            fix(v)
    return o

json.dump(fix(json.load(open(sys.argv[1]))), sys.stdout, indent=1, ensure_ascii=False)
