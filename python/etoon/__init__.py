"""etoon: fast TOON encoder for Python.

Bridges Python → orjson (JSON bytes) → Rust → TOON string.
"""

from typing import Any

import orjson

from etoon._etoon import dumps_bytes as _dumps_bytes

__version__ = "0.1.0"
__all__ = ["dumps"]


def dumps(data: Any) -> str:
    """Encode a Python value to TOON format (2-space indent).

    Accepts anything orjson can serialize (dict, list, str, int, float,
    bool, None, datetime, UUID, etc.). Falls back to stdlib json for
    integers that exceed 64-bit range.
    """
    if isinstance(data, (bytes, bytearray)):
        return _dumps_bytes(bytes(data))
    try:
        json_bytes = orjson.dumps(data)
    except TypeError:
        # orjson rejects ints > 2**63-1; stdlib json handles arbitrary ints.
        import json as _stdlib_json
        json_bytes = _stdlib_json.dumps(data, ensure_ascii=False).encode("utf-8")
    return _dumps_bytes(json_bytes)
