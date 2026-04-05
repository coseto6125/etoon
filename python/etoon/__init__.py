"""etoon: fast TOON encoder for Python.

Bridges Python → orjson (JSON bytes) → Rust → TOON string.
"""

from typing import Any, Literal

import orjson

from etoon._etoon import dumps_bytes as _dumps_bytes

__version__ = "0.1.2"
__all__ = ["dumps"]

Delimiter = Literal[",", "\t", "|"]


def dumps(
    data: Any,
    *,
    delimiter: Delimiter = ",",
    fold_keys: bool = False,
    flatten_depth: int | None = None,
) -> str:
    """Encode a Python value to TOON format (2-space indent).

    Accepts anything orjson can serialize (dict, list, str, int, float,
    bool, None, datetime, UUID, etc.). Falls back to stdlib json for
    integers that exceed 64-bit range.

    Args:
        data: The value to encode.
        delimiter: Separator for array/tabular values: ``","``, ``"\\t"``, or ``"|"``.
        fold_keys: If True, collapse single-key object chains into dot-notation
            keys (``{"a": {"b": 1}}`` → ``"a.b: 1"``). Safe mode: skips folding
            when a segment needs quoting, and avoids collisions with sibling keys.
        flatten_depth: Maximum chain length when ``fold_keys=True``. ``None``
            means unlimited; ``0`` disables folding.
    """
    if isinstance(data, (bytes, bytearray)):
        return _dumps_bytes(bytes(data), delimiter, fold_keys, flatten_depth)
    try:
        json_bytes = orjson.dumps(data)
    except TypeError:
        # orjson rejects ints > 2**63-1; stdlib json handles arbitrary ints.
        import json as _stdlib_json

        json_bytes = _stdlib_json.dumps(data, ensure_ascii=False).encode("utf-8")
    return _dumps_bytes(json_bytes, delimiter, fold_keys, flatten_depth)
