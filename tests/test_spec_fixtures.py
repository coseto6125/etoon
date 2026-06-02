"""
Run the TOON spec fixtures against etoon.

Encode fixtures track the official `toon-format/spec` repo (TOON spec v3.1).
`key-folding.json` additionally carries etoon's `@`/`$`/`#` sigil-prefix cases.
See ATTRIBUTION.md. Each fixture file contains N test cases with (input, expected) pairs.
"""

import pathlib

import etoon
import orjson
import pytest

FIXTURES_DIR = pathlib.Path(__file__).parent / "fixtures" / "encode"

# Fixtures we currently support (tier 1 MVP — no delimiters, no key-folding)
SUPPORTED_FIXTURES = {
    "primitives.json",
    "objects.json",
    "arrays-primitive.json",
    "arrays-tabular.json",
    "arrays-objects.json",
    "arrays-nested.json",
    "whitespace.json",
    "delimiters.json",
    "key-folding.json",
}


def _collect_cases():
    cases = []
    for fixture_file in sorted(FIXTURES_DIR.glob("*.json")):
        if fixture_file.name not in SUPPORTED_FIXTURES:
            continue
        data = orjson.loads(fixture_file.read_bytes())
        for test in data["tests"]:
            opts = test.get("options") or {}
            # Skip tests requiring non-default indent; we hardcode 2-space.
            if opts.get("indent", 2) != 2:
                continue
            kwargs = {}
            if "delimiter" in opts:
                kwargs["delimiter"] = opts["delimiter"]
            if opts.get("keyFolding") == "safe":
                kwargs["fold_keys"] = True
                if "flattenDepth" in opts:
                    kwargs["flatten_depth"] = opts["flattenDepth"]
            cases.append(
                pytest.param(
                    test["input"],
                    test["expected"],
                    kwargs,
                    id=f"{fixture_file.stem}::{test['name']}",
                )
            )
    return cases


@pytest.mark.parametrize(("payload", "expected", "kwargs"), _collect_cases())
def test_encode_matches_spec(payload, expected, kwargs):
    got = etoon.dumps(payload, **kwargs)
    assert got == expected
