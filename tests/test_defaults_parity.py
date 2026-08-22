"""
Behavior-level parity between the two option-definition sites.

The options interface is defined twice by necessity, once per language:
the ``dumps`` kwargs (Python truth) and ``Config::default`` (Rust truth).
Two layers of probes pin them together through observable behavior only:

1. ``dumps`` level — omitting an option equals passing its documented
   default explicitly. Pins the Python signature.
2. ``dumps_bytes`` level — calling the raw binding with NO kwargs equals
   passing its documented default explicitly, on payloads chosen so each
   option changes the output. This is the only path that exercises
   ``Config::default``, so it is the only one that can catch Rust-side
   default drift.

If either side drifts, an equality fails — except ``flatten_depth``,
which zero kwargs cannot observe: at the defaults ``key_folding`` is off,
so folding never runs. It is pinned only by the ``dumps`` probe above.
"""

import orjson
import pytest
from etoon import dumps

# bytes input so the guards are live on every call
PAYLOAD = orjson.dumps({"t": [1, 2, 3], "k": "\x01"})


def _dumps_bytes():
    # the raw binding IS the probe target: zero kwargs exercises Config::default
    from etoon._etoon import dumps_bytes  # ruff: ignore[import-private-name]

    return dumps_bytes


def _all_defaults():
    return {
        "delimiter": ",",
        "key_folding": False,
        "flatten_depth": None,
        "empty_array_bare": True,
        "escape_controls": True,
        "max_depth": 1000,
        "max_input_bytes": 0,
    }


# ---- layer 1: Python signature defaults (dumps) ----


def test_dumps_default_delimiter_is_comma():
    x = {"t": [1, 2, 3]}
    assert dumps(x) == dumps(x, delimiter=",")


def test_dumps_default_fold_keys_is_false():
    x = {"a": {"b": 1}}
    assert dumps(x) == dumps(x, fold_keys=False)


def test_dumps_default_flatten_depth_is_unlimited():
    x = {"a": {"b": {"c": 1}}}
    got = dumps(x, fold_keys=True)
    assert got == dumps(x, fold_keys=True, flatten_depth=None)
    assert got == "a.b.c: 1"


def test_dumps_default_empty_array_bare_is_true():
    x = {"k": []}
    assert dumps(x) == dumps(x, empty_array_bare=True)


def test_dumps_default_escape_controls_is_true():
    x = {"k": "\x01"}
    assert dumps(x) == dumps(x, escape_controls=True)


def test_dumps_default_max_depth_is_1000():
    def nested(n):
        return ('{"a":' * n + "1" + "}" * n).encode()

    with pytest.raises(ValueError, match="max_depth"):
        dumps(nested(1001))
    assert isinstance(dumps(nested(1000)), str)


def test_dumps_default_max_input_bytes_is_unlimited():
    big = b'{"k":"' + b"x" * 200_000 + b'"}'
    assert isinstance(dumps(big), str)


# ---- layer 2: Rust Config::default (raw binding, zero kwargs) ----


def test_binding_no_kwargs_equals_all_explicit_defaults():
    db = _dumps_bytes()
    assert db(PAYLOAD) == db(PAYLOAD, **_all_defaults())


def test_binding_rust_default_delimiter():
    db = _dumps_bytes()
    assert db(PAYLOAD) == db(PAYLOAD, delimiter=",")


def test_binding_rust_default_escape_controls():
    db = _dumps_bytes()
    assert db(PAYLOAD) == db(PAYLOAD, escape_controls=True)


def test_binding_rust_default_max_input_bytes_disabled():
    big = b'{"k":"' + b"x" * 200_000 + b'"}'
    # a nonzero default would reject this; disabled accepts it like the
    # explicit default does
    assert isinstance(_dumps_bytes()(big), type(_dumps_bytes()(big, max_input_bytes=0)))


def test_binding_rust_default_key_folding_is_false():
    # payload must nest an object chain, or fold on/off emits identical bytes
    db = _dumps_bytes()
    x = orjson.dumps({"a": {"b": 1}})
    assert db(x) == db(x, key_folding=False) == "a:\n  b: 1"


def test_binding_rust_default_empty_array_bare_is_true():
    # an array value is required: scalars look identical under either setting
    db = _dumps_bytes()
    x = orjson.dumps({"k": []})
    assert db(x) == db(x, empty_array_bare=True) == "k: []"


def test_binding_rust_default_max_depth_is_1000():
    # drift in either direction trips this: lower rejects depth 1000, higher
    # (or 0 = disabled) accepts depth 1001
    db = _dumps_bytes()

    def nested(n):
        return ('{"a":' * n + "1" + "}" * n).encode()

    assert isinstance(db(nested(1000)), str)
    with pytest.raises(ValueError, match="max_depth"):
        db(nested(1001))


def test_binding_rejects_unknown_kwargs():
    with pytest.raises(TypeError, match="unexpected keyword"):
        _dumps_bytes()(b"{}", nonsense=True)
