"""
Compare etoon against other TOON encoders: speed and output parity.

Encoders measured (each auto-skipped if not importable):
  - etoon      (this package, Rust/PyO3)
  - toons      (reference Python encoder, Apache 2.0 — alesanfra/toons)
  - py-rtoon   (Rust-backed Python encoder — batprem/py-rtoon)
  - TS SDK     (@toon-format/toon, via the benches/ts_bench.mjs Node sidecar)

For each payload it reports the best-of-N per-call time, the speedup vs etoon,
and whether each encoder's output is byte-identical to etoon.

Usage:
    python benches/compare.py            # default payloads
    python benches/compare.py --iters 5000
"""

import argparse
import json
import pathlib
import subprocess
import time

import etoon
import orjson

HERE = pathlib.Path(__file__).parent


def make_payloads() -> dict[str, object]:
    """Build representative shapes: wide tabular, deep nested, string-heavy, mixed."""
    return {
        "tabular_1000": [
            {"id": i, "name": f"item_{i}", "price": i * 1.5, "active": i % 2 == 0}
            for i in range(1000)
        ],
        "nested_deep": {
            "root": {f"k{i}": {"data": [{"id": j} for j in range(20)]} for i in range(100)}
        },
        "strings_1000": [{"text": f"hello, world {i}!", "tag": f"x{i}"} for i in range(1000)],
        "mixed_500": [
            {"id": i, "meta": {"ok": True, "score": i * 0.1}, "tags": ["a", "b"]}
            for i in range(500)
        ],
        # v4.1 collapsing forms: nested field groups (§9.3) and keyed tabular
        # (§9.5). Encoders still on spec v3.x expand these into nested blocks.
        "groups_1000": [
            {"id": i, "customer": {"name": f"n{i}", "country": "DK"}, "total": i}
            for i in range(1000)
        ],
        "keyed_1000": {
            f"e{i}": {"host": f"h{i}.example.com", "port": 8000 + i}
            for i in range(1000)
        },
    }


def bench(fn, data, iters: int, rounds: int) -> float:
    """Best-of-`rounds` average per-call time in microseconds."""
    for _ in range(3):
        fn(data)
    best = float("inf")
    for _ in range(rounds):
        t0 = time.perf_counter()
        for _ in range(iters):
            fn(data)
        best = min(best, (time.perf_counter() - t0) / iters)
    return best * 1e6


def load_py_encoders() -> dict[str, callable]:
    """Map of available reference encoders, keyed by display name."""
    encoders = {}
    try:
        import toons

        encoders["toons"] = toons.dumps
    except ImportError:
        pass
    try:
        import py_rtoon

        encoders["py-rtoon"] = py_rtoon.encode_default
    except ImportError:
        pass
    return encoders


def find_cli() -> pathlib.Path | None:
    """Locate the built etoon CLI binary, or None if it hasn't been built."""
    candidate = HERE.parent / "target" / "release" / "etoon"
    return candidate if candidate.exists() else None


def bench_cli(cli: pathlib.Path, json_bytes: bytes, iters: int, rounds: int) -> tuple[float, str]:
    """
    Best-of-N per-call time (µs) for spawning the CLI once per encode.

    This measures the real `… | etoon` cost: process spawn + stdin read +
    encode + stdout write, NOT the in-process PyO3 path. Returns (µs, output).
    """
    def one() -> bytes:
        return subprocess.run(  # noqa: S603 — fixed argv, trusted local binary
            [str(cli)],
            input=json_bytes,
            capture_output=True,
            check=True,
        ).stdout

    for _ in range(2):
        one()
    best = float("inf")
    for _ in range(rounds):
        t0 = time.perf_counter()
        for _ in range(iters):
            one()
        best = min(best, (time.perf_counter() - t0) / iters)
    # CLI appends a trailing newline; strip it for parity with dumps().
    return best * 1e6, one().decode().rstrip("\n")


def run_ts_sidecar(payloads: dict, iters: int, rounds: int) -> dict | None:
    """Encode each payload with the TS SDK via Node; None if unavailable."""
    sidecar = HERE / "ts_bench.mjs"
    if not sidecar.exists():
        return None
    req = [
        {"name": n, "value": v, "iters": iters, "rounds": rounds}
        for n, v in payloads.items()
    ]
    try:
        proc = subprocess.run(  # noqa: S603 — fixed argv, trusted local sidecar
            ["node", str(sidecar)],  # noqa: S607 — `node` resolved from PATH by design
            input=orjson.dumps(req),
            capture_output=True,
            timeout=120,
            check=False,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None
    if proc.returncode != 0:
        return None
    return json.loads(proc.stdout)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--iters", type=int, default=2000)
    ap.add_argument("--rounds", type=int, default=7)
    ap.add_argument(
        "--cli-iters",
        type=int,
        default=30,
        help="spawns per round for the etoon-cli column (process spawn is costly)",
    )
    args = ap.parse_args()

    payloads = make_payloads()
    py_encoders = load_py_encoders()
    ts = run_ts_sidecar(payloads, args.iters, args.rounds)
    cli = find_cli()

    for name in ("toons", "py-rtoon"):
        if name not in py_encoders:
            print(f"note: `{name}` not installed — skipping")
    if ts is None:
        print("note: TS SDK sidecar unavailable (no node / @toon-format/toon) — skipping")
    if cli is None:
        print("note: etoon CLI binary not built — skipping (run `cargo build --release --bin etoon`)")
    print(f"\netoon {etoon.__version__}  |  iters={args.iters} rounds={args.rounds}\n")

    cols = ["etoon", *py_encoders.keys()]
    if ts is not None:
        cols.append("TS")
    if cli is not None:
        cols.append("etoon-cli")
    width = 22
    header = f"{'payload':14} " + " ".join(f"{c:>{width}}" for c in cols)
    print(header)
    print("-" * len(header))

    for name, data in payloads.items():
        e_out = etoon.dumps(data)
        e_us = bench(etoon.dumps, data, args.iters, args.rounds)
        # etoon column: absolute time only (it is the 1.00x baseline).
        cells = {"etoon": f"{e_us:8.1f}µs"}

        for enc_name, fn in py_encoders.items():
            us = bench(fn, data, args.iters, args.rounds)
            parity = "✓" if fn(data) == e_out else "✗"
            # Each cell shows the encoder's own time AND its slowdown vs etoon.
            cells[enc_name] = f"{us:8.1f}µs ({us / e_us:.1f}x{parity})"

        if ts is not None and name in ts:
            us = ts[name]["us"]
            parity = "✓" if ts[name]["output"] == e_out else "✗"
            cells["TS"] = f"{us:8.1f}µs ({us / e_us:.1f}x{parity})"

        if cli is not None:
            # Per-call subprocess spawn — the real `… | etoon` shell pipe cost.
            cli_us, cli_out = bench_cli(cli, orjson.dumps(data), args.cli_iters, args.rounds)
            parity = "✓" if cli_out == e_out else "✗"
            cells["etoon-cli"] = f"{cli_us:8.1f}µs ({cli_us / e_us:.0f}x{parity})"

        row = f"{name:14} " + " ".join(f"{cells.get(c, 'n/a'):>{width}}" for c in cols)
        print(row)

    print("\nEach cell: absolute best-of-N time, then (slowdown vs etoon PyO3, parity).")
    print("✓ byte-identical to etoon · ✗ differs (deviates from etoon's spec-canonical")
    print("output, e.g. py-rtoon emits `0.0` where the TOON spec requires `0`).")
    print("etoon-cli = per-call `subprocess.run(etoon)`: includes process spawn + pipe")
    print(f"I/O ({args.cli_iters} spawns/round), so it is dominated by startup, not encoding.")


if __name__ == "__main__":
    main()
