# etoon

Fast [TOON](https://github.com/toon-format/toon) (Token-Oriented Object Notation) encoder for Python, Rust, and CLI.

**8× faster than `toons`**, **2.7× faster than the official TS SDK**, byte-identical output.

[中文說明](./README.zh-TW.md)

## Performance

Measured on a 50-doc "neptune" payload (7480 bytes JSON → 4012 bytes TOON):

| Encoder               | Time    | Relative |
|-----------------------|---------|----------|
| **etoon (Python)**    | 13.0 μs | **1.00×** |
| @toon-format/toon (TS)| 35.6 μs | 2.73×    |
| py-rtoon              | 85.9 μs | 6.59×    |
| toons                 | 106.4 μs| 8.17×    |

**CLI via stdin pipe** (Claude / Bash workflows):

| CLI           | Per call | Relative |
|---------------|----------|----------|
| **etoon**     | 0.57 ms  | **1.00×** |
| official toon | 50.7 ms  | 89× slower |

## Install

### Python
```bash
pip install etoon
```

### Rust library
```bash
cargo add etoon --no-default-features
```

### CLI binary
Download from [GitHub Releases](https://github.com/coseto6125/etoon/releases), or:
```bash
cargo install etoon
```

## Usage

### Python
```python
import etoon
docs = [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]
print(etoon.dumps(docs))
# [2]{id,name}:
#   1,Alice
#   2,Bob
```

### CLI (Bash pipe)
```bash
curl -s https://api.example.com/data | etoon
cat data.json | etoon -o output.toon
```

### Rust
```rust
let json_bytes = serde_json::to_vec(&my_data)?;
let toon = etoon::toon::encode(&json_bytes)?;
```

## Architecture

```
Python dict → orjson.dumps → JSON bytes → sonic-rs (SIMD parse) → walk → TOON string
```

Key optimizations:
- **sonic-rs SIMD JSON parser** (~7× faster than serde_json)
- **orjson bridge** — single boundary crossing (vs PyO3-based alternatives)
- **uniform-order table fast path** — skips 300 key lookups per 50-row table
- **itoa specialized integer formatting**

## Compatibility

Output is byte-identical to the `toons` Python package (Apache 2.0) and the
official `toon-format/toon` TypeScript SDK. Passes **111/111** TOON spec
fixtures covering primitives, objects, arrays (primitive/tabular/nested/bulleted),
and whitespace.

## Limitations

- Integers > 2⁶³ are lossily coerced via f64 (works for most common big integers
  that happen to be representable; arbitrary-precision is not supported).
- Custom `indent` is hardcoded to 2 spaces (TOON spec default).
- TOON-specific features not yet supported: `--fold-keys`, `--delimiter tab/pipe`.

## License

Apache 2.0. Test fixtures in `tests/fixtures/` are sourced from the
[toons](https://github.com/alesanfra/toons) project (Apache 2.0).
