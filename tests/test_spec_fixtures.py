"""
Run the TOON spec fixtures against etoon.

Encode fixtures are the official `toon-format/spec` suite (TOON spec v4.4.0).
`spec-v4.1-legacy.json` keeps the v4.1.1 cases whose input, options or expected
output the v4.4.0 suite no longer contains.
`key-folding.json` is etoon-local: the spec dropped key folding in v4.0, so it
guards etoon's own `fold_keys` extension plus the `@`/`$`/`#` sigil-prefix cases.
See ATTRIBUTION.md. Each fixture file contains N test cases with (input, expected) pairs.
"""

import pathlib

import etoon
import orjson
import pytest

FIXTURES_DIR = pathlib.Path(__file__).parent / "fixtures" / "encode"

SUPPORTED_FIXTURES = {
    "primitives.json",
    "objects.json",
    "objects-keyed.json",
    "arrays-primitive.json",
    "arrays-tabular.json",
    "arrays-objects.json",
    "arrays-nested.json",
    "whitespace.json",
    "delimiters.json",
    "key-folding.json",
    "spec-v4.1-legacy.json",
}


def _collect_cases():
    cases = []
    for fixture_file in sorted(FIXTURES_DIR.glob("*.json")):
        if fixture_file.name not in SUPPORTED_FIXTURES:
            continue
        data = orjson.loads(fixture_file.read_bytes())
        for test in data["tests"]:
            opts = test.get("options") or {}
            # `indent` is the pre-v3.3 spelling, kept for the local fixture.
            indent = opts.get("indentSize", opts.get("indent", 2))
            marks = (
                pytest.mark.skip(reason=f"etoon hardcodes indentSize 2, case needs {indent}")
                if indent != 2
                else ()
            )
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
                    marks=marks,
                )
            )
    return cases


@pytest.mark.parametrize(("payload", "expected", "kwargs"), _collect_cases())
def test_encode_matches_spec(payload, expected, kwargs):
    got = etoon.dumps(payload, **kwargs)
    assert got == expected
