"""
etoon: fast TOON encoder for Python.

Bridges Python → orjson (JSON bytes) → Rust → TOON string.
"""

from typing import Any, Literal

import orjson

from etoon._etoon import dumps_bytes as _dumps_bytes

__version__ = "0.7.2"
__all__ = ["dumps"]

Delimiter = Literal[",", "\t", "|"]


def dumps(
    data: Any,
    *,
    delimiter: Delimiter = ",",
    fold_keys: bool = False,
    flatten_depth: int | None = None,
    empty_array_bare: bool = True,
    escape_controls: bool = True,
    max_depth: int = 1000,
    max_input_bytes: int = 0,
) -> str:
    r"""
    Encode a Python value to TOON format (2-space indent).

    Accepts anything orjson can serialize (dict, list, str, int, float,
    bool, None, datetime, UUID, etc.). Falls back to stdlib json for
    integers that exceed 64-bit range.

    Args:
        data: The value to encode.
        delimiter: Separator for array/tabular values: ``","``, ``"\\t"``, or ``"|"``.
        fold_keys: If True, collapse single-key object chains into dot-notation
            keys (``{"a": {"b": 1}}`` → ``"a.b: 1"``). Safe mode: skips folding
            when a segment needs quoting, and avoids collisions with sibling keys.
            An etoon extension: the spec dropped key folding in v4.0, so folded
            output is still valid TOON (dotted keys are literal keys) but no
            decoder re-nests it.
        flatten_depth: Maximum chain length when ``fold_keys=True``. ``None``
            means unlimited; ``0`` disables folding.
        empty_array_bare: If True (default), emit empty arrays as canonical
            ``[]`` / ``key: []`` instead of the legacy ``[0]:`` form. A bare
            array *element* that is itself empty (e.g. ``[[], []]``) always
            keeps ``- [0]:`` per spec §9.2; object fields use ``key: []``.
            Setting it False produces output the spec forbids since v3.1.
        escape_controls: If True (default), escape control chars U+0000–U+001F
            (except ``\n`` ``\r`` ``\t``) as ``\uXXXX`` with lowercase hex.
            Setting it False produces output the spec forbids since v3.1.
        max_depth: Maximum JSON nesting depth for **raw bytes/bytearray input**
            (default ``1000``). Input nested deeper is rejected with
            ``ValueError`` before parsing, guarding against a stack overflow
            that would otherwise crash the process. Ignored for dict/list input,
            whose depth is already bounded by orjson + CPython's recursion limit
            (so the pre-scan would be redundant overhead on the hot path).
        max_input_bytes: Maximum size in bytes for **raw bytes/bytearray input**;
            ``0`` (default) disables the check. Set this to bound peak memory on
            untrusted byte input. Ignored for dict/list input (orjson has already
            allocated the serialized bytes by the time encoding runs).
    """
    # One named construction point for every option: keyword arguments at each
    # call site cannot be misrouted if a new option is inserted later.
    opts = {
        "delimiter": delimiter,
        "key_folding": fold_keys,
        "flatten_depth": flatten_depth,
        "empty_array_bare": empty_array_bare,
        "escape_controls": escape_controls,
        "max_depth": max_depth,
        "max_input_bytes": max_input_bytes,
    }
    if isinstance(data, bytes):
        return _dumps_bytes(data, **opts)
    if isinstance(data, bytearray):
        return _dumps_bytes(bytes(data), **opts)
    try:
        json_bytes = orjson.dumps(data)
    except TypeError:
        # orjson rejects ints > 2**63-1; stdlib json handles arbitrary ints.
        import json as _stdlib_json

        json_bytes = _stdlib_json.dumps(data, ensure_ascii=False).encode("utf-8")
    # orjson/stdlib output is already depth-bounded by CPython's recursion
    # limit, so skip the depth/size pre-scan (max_depth=0) on this hot path.
    opts["max_depth"] = 0
    opts["max_input_bytes"] = 0
    return _dumps_bytes(json_bytes, **opts)
