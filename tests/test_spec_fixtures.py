"""Run the TOON spec fixtures against etoon.

Fixtures sourced from the `toons` project (Apache 2.0). See ATTRIBUTION.md.
Each fixture file contains N test cases with (input, expected) pairs.
"""

import pathlib

import orjson
import pytest

import etoon

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
}


def _collect_cases():
    cases = []
    for fixture_file in sorted(FIXTURES_DIR.glob("*.json")):
        if fixture_file.name not in SUPPORTED_FIXTURES:
            continue
        data = orjson.loads(fixture_file.read_bytes())
        for test in data["tests"]:
            # Skip tests requiring non-default indent; we hardcode 2-space.
            if (test.get("options") or {}).get("indent", 2) != 2:
                continue
            cases.append(
                pytest.param(
                    test["input"],
                    test["expected"],
                    id=f"{fixture_file.stem}::{test['name']}",
                )
            )
    return cases


@pytest.mark.parametrize("payload,expected", _collect_cases())
def test_encode_matches_spec(payload, expected):
    got = etoon.dumps(payload)
    assert got == expected
