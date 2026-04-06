# etoon

Fast [TOON](https://github.com/toon-format/toon) (Token-Oriented Object Notation) encoder for Python, Rust, and CLI.

**8× faster than `toons`**, **2.7× faster than the official TS SDK**, byte-identical output.

[中文說明](https://github.com/coseto6125/etoon/blob/main/README.zh-TW.md)

## Performance

Measured on a 50-doc payload (7480 bytes JSON → 4012 bytes TOON):

| Encoder                    | Time    | vs etoon |
|----------------------------|---------|----------|
| **etoon (Rust, native)**   | 12.1 μs | **1.00×** |
| **etoon (Python, PyO3)**   | 15.4 μs | 1.27×    |
| @toon-format/toon (TS SDK) | 35.6 μs | 2.94×    |
| py-rtoon                   | 85.9 μs | 7.10×    |
| toons                      | 106.4 μs| 8.79×    |

**CLI via stdin pipe** (Claude / Bash workflows):

| CLI           | Per call | Relative |
|---------------|----------|----------|
| **etoon**     | 0.43 ms  | **1.00×** |
| official toon | 50.7 ms  | 118× slower |

**Auto-detect mode** (v0.2.0+) — handles JSON, mixed log, and plain text:

| Input                          | Size  | Per call |
|--------------------------------|-------|----------|
| Pure JSON (1000 objects)       | 120KB | 0.73 ms  |
| Mixed log (5K JSON + 5K text) | 600KB | 1.93 ms  |
| Plain text pass-through        | 300KB | 0.56 ms  |

### Reproduce

```bash
# Encoder core benchmark (Rust native, no I/O)
cargo run --release --bin bench payload.json

# CLI stdin pipe benchmark
python3 -c "
import json
data = [{'id': i, 'name': f'item_{i}', 'price': i*1.5, 'tags': ['a','b','c']} for i in range(1000)]
print(json.dumps(data))
" > /tmp/bench.json

# Time 200 runs
start=$(date +%s%N)
for i in $(seq 1 200); do etoon < /tmp/bench.json > /dev/null; done
end=$(date +%s%N)
echo "$(echo "scale=2; ($end - $start) / 200000000" | bc)ms avg"
```

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
aws logs tail /ecs/my-service | etoon        # mixed log: JSON blocks → TOON, text → pass-through
etoon --strict < data.json                   # error on non-JSON (old behavior)
```

### LLM Agents — Setup Guide

Append `| etoon` to shell commands so the AI reads TOON instead of raw JSON — **40-60% fewer tokens**, lossless, auto-detects input format.

```bash
gh pr list --json number,title,author | etoon
aws logs tail /ecs/my-service | etoon
kubectl get pods -o json | etoon
```

**Rules**:
- `| etoon` goes at the **end** of the pipeline — `jq` / `grep` / `awk` before it, never after
- Auto-detects JSON / mixed log / plain text — safe to append to **any** command
- Skip only for: byte-exact diff, single-value lookups (1-2 lines)

---

#### Claude Code

Add to `CLAUDE.md` (or `.claude/CLAUDE.md`) in your project root:

````markdown
## Bash Output

Append `| etoon` as the final pipe segment of any Bash command whose output
reaches the conversation. etoon auto-detects JSON, mixed log, and plain text.
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### Cursor

Add to `.cursor/rules/etoon.mdc`:

````markdown
---
description: Compress shell output with etoon
globs:
alwaysApply: true
---

When running shell/terminal commands, append `| etoon` as the last pipe segment.
etoon auto-detects JSON, mixed log, and plain text — safe for any command.
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### OpenAI Codex CLI

Add to `AGENTS.md` in your project root:

````markdown
## Bash Output

Append `| etoon` as the final pipe segment of any shell command.
etoon auto-detects JSON, mixed log, and plain text (non-JSON passes through unchanged).
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### Windsurf / Other AI Editors

Add to `.windsurfrules` (Windsurf) or the equivalent rules file:

````markdown
When executing shell commands, always append `| etoon` at the end of the pipeline.
etoon auto-detects input format: JSON → compact TOON, mixed log → JSON blocks encoded + text pass-through, plain text → unchanged.
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### Gemini CLI

Add to `GEMINI.md` in your project root:

````markdown
## Bash Output

Append `| etoon` as the final pipe segment of any shell command.
etoon auto-detects JSON, mixed log, and plain text (non-JSON passes through unchanged).
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### ChatGPT / Custom GPTs

Add to system prompt or custom instructions:

```
When generating shell commands for the user, append `| etoon` as the last pipe segment.
etoon converts JSON to TOON (40-60% fewer tokens). Non-JSON passes through unchanged.
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

## Advanced options

```python
# Custom delimiter (saves tokens when values contain commas)
etoon.dumps(data, delimiter="|")   # or "\t"

# Key folding: collapse {a:{b:{c:1}}} → "a.b.c: 1"
etoon.dumps(data, fold_keys=True)
etoon.dumps(data, fold_keys=True, flatten_depth=2)  # partial fold
```

## Limitations

- Integers > 2⁶³ are lossily coerced via f64 (works for most common big integers
  that happen to be representable; arbitrary-precision is not supported).
- Custom `indent` is hardcoded to 2 spaces (TOON spec default).

## License

Apache 2.0. Test fixtures in `tests/fixtures/` are sourced from the
[toons](https://github.com/alesanfra/toons) project (Apache 2.0).
