r"""
Malformed delimiter must raise ValueError, never fall back to comma.

The PyO3 adapter used to keep only the first byte of ``delimiter``, so
``""`` and ``",,"`` silently encoded with ``,``. These tests pin the
contract: exactly ",", "\\t", "|" are accepted, everything else raises.
"""

import pytest
from etoon import dumps


@pytest.mark.parametrize("bad", ["", ",,", "x", "|,", "\t\t"])
def test_dumps_malformed_delimiter_raises(bad):
    with pytest.raises(ValueError, match="delimiter must be"):
        dumps({"a": 1}, delimiter=bad)


@pytest.mark.parametrize("good", [",", "\t", "|"])
def test_dumps_valid_delimiters_accepted(good):
    assert isinstance(dumps({"a": 1}, delimiter=good), str)
