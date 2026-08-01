# etoon

[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/coseto6125/etoon/badge)](https://scorecard.dev/viewer/?uri=github.com/coseto6125/etoon)
[![SLSA 3](https://slsa.dev/images/gh-badge-level3.svg)](https://slsa.dev)
[![VirusTotal](https://img.shields.io/badge/VirusTotal-scanned-brightgreen?logo=virustotal)](https://github.com/coseto6125/etoon/releases)
[![cargo audit](https://img.shields.io/badge/cargo_audit-passing-brightgreen?logo=rust)](https://rustsec.org)

Fast [TOON](https://github.com/toon-format/toon) (Token-Oriented Object Notation) encoder for Python, Rust, and CLI.

**Up to 7.6× faster than `toons`**, **3.0–8.3× faster than the official TS SDK**, byte-identical output — tracking **TOON spec v4.1**.

[中文說明](https://github.com/coseto6125/etoon/blob/main/README.zh-TW.md)

## Performance

Per-call encode time across representative payloads (`etoon` = Python/PyO3,
best-of-7 × 400 calls). `✓` = output byte-identical to etoon; `✗` = the encoder
differs — either it deviates from the spec (py-rtoon emits `0.0` where the spec
requires `0`) or it still implements spec v3.x and expands the v4.1 collapsing
forms into nested blocks.

| Payload (encode)         | etoon   | toons           | py-rtoon        | @toon-format/toon 4.1 (TS) |
|--------------------------|---------|-----------------|-----------------|----------------------------|
| 1000 uniform objects     | 171 µs  | 848 µs (5.0×✓)  | 772 µs (4.5×✗)  | 519 µs (3.0×✓)             |
| deep nested              | 110 µs  | 272 µs (2.5×✓)  | 662 µs (6.0×✗)  | 611 µs (5.6×✓)             |
| 1000 string records      | 89 µs   | 674 µs (7.6×✓)  | 540 µs (6.1×✓)  | 640 µs (7.2×✓)             |
| 500 mixed objects        | 140 µs  | 680 µs (4.8×✓)  | 541 µs (3.9×✗)  | 1166 µs (8.3×✓)            |
| 1000 nested field groups | 190 µs  | 1103 µs (5.8×✗) | 953 µs (5.0×✗)  | 586 µs (3.1×✓)             |
| 1000 keyed-tabular rows  | 105 µs  | 679 µs (6.5×✗)  | 562 µs (5.4×✗)  | 603 µs (5.8×✓)             |

**2.5–8.3× faster** than every other encoder, with **byte-identical,
spec-canonical** output: only the official TS SDK matches etoon byte-for-byte on
all six payloads.

The CLI (`… | etoon`) adds process-spawn + pipe I/O on top — fine for shell
pipelines / LLM logs, but for in-process use prefer the PyO3 `dumps` (no spawn,
no pipe). Auto-detect mode (JSON / mixed log / plain text) runs at ~0.6–1.9 ms
per call on 100–600 KB inputs.

### Reproduce

```bash
# Speed + parity vs toons / py-rtoon / TS SDK (each auto-skipped if absent):
pip install -e '.[bench]'        # toons + py-rtoon
npm install @toon-format/toon    # optional: TS SDK comparison
python benches/compare.py        # add --iters N to tune

# Encoder core benchmark (Rust native, no Python/PyO3 overhead):
cargo run --release --bin bench payload.json
```

## Install

### CLI binary (recommended for LLM workflows)

**Pre-built — no Rust required:**

Download from [GitHub Releases](https://github.com/coseto6125/etoon/releases) (Linux/macOS/Windows, x86_64/aarch64):

<details>
<summary><b>Linux</b></summary>

```bash
# x86_64
curl -L https://github.com/coseto6125/etoon/releases/latest/download/etoon-linux-x86_64 -o etoon

# Apple Silicon / ARM server (aarch64)
curl -L https://github.com/coseto6125/etoon/releases/latest/download/etoon-linux-aarch64 -o etoon

chmod +x etoon
sudo mv etoon /usr/local/bin/   # or ~/.local/bin/
```
</details>

<details>
<summary><b>macOS</b></summary>

```bash
# Apple Silicon (M1/M2/M3/M4)
curl -L https://github.com/coseto6125/etoon/releases/latest/download/etoon-macos-aarch64 -o etoon

# Intel Mac
curl -L https://github.com/coseto6125/etoon/releases/latest/download/etoon-macos-x86_64 -o etoon

chmod +x etoon
sudo mv etoon /usr/local/bin/
```
</details>

<details>
<summary><b>Windows</b></summary>

```powershell
# PowerShell
Invoke-WebRequest -Uri "https://github.com/coseto6125/etoon/releases/latest/download/etoon-windows-x86_64.exe" -OutFile "etoon.exe"

# Move to a directory in your PATH, e.g.:
Move-Item etoon.exe "$env:USERPROFILE\.local\bin\etoon.exe"
```
</details>

<details>
<summary><b>Verify download (optional)</b></summary>

Each release includes SHA256 checksums, [SLSA provenance](https://slsa.dev) attestation, and [VirusTotal](https://www.virustotal.com) scan reports.

```bash
# 1. Verify checksum
curl -L https://github.com/coseto6125/etoon/releases/latest/download/SHA256SUMS.txt -o SHA256SUMS.txt
sha256sum -c SHA256SUMS.txt --ignore-missing

# 2. Verify SLSA provenance (requires gh CLI)
gh attestation verify etoon-linux-x86_64 --repo coseto6125/etoon

# 3. VirusTotal — scan report links are in the release notes
```

macOS unsigned binary note: `xattr -d com.apple.quarantine etoon` to bypass Gatekeeper.
</details>

**From source (requires Rust toolchain):**

```bash
cargo install etoon
```

### Python library

```bash
pip install etoon
```

> This installs the Python binding (`etoon.dumps()`), **not** the CLI binary. For the CLI, use one of the methods above.

### Rust library

```bash
cargo add etoon --no-default-features
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
- **first-row column probe** — an array or empty-object value rules out tabular
  form from one element alone, so mixed arrays reach list form in O(columns)
- **itoa specialized integer formatting**

## Compatibility

Tracks **TOON spec v4.1**. Output is byte-identical to the official
`toon-format/toon` TypeScript SDK 4.1, and passes **178/179** cases of the
official [`toon-format/spec`](https://github.com/toon-format/spec) encode
fixture suite — every case except one requiring a non-default `indentSize`
(etoon hardcodes 2 spaces).

### v4 collapsing forms

Spec v4.0 added two forms that cut nesting out of common shapes, both
implemented here:

```bash
# Nested field groups (§9.3) — uniform nested objects become header columns
echo '[{"id":1,"customer":{"name":"Ada","country":"DK"},"total":99}]' | etoon
# orders[1]{id,customer{name,country},total}:
#   1,Ada,DK,99

# Keyed tabular (§9.5) — an object of uniform objects becomes a keyed table
echo '{"alpha":{"host":"a.example.com","port":8080},"beta":{"host":"b.example.com","port":9090}}' | etoon
# [2:]{host,port}:
#   alpha: a.example.com,8080
#   beta: b.example.com,9090
```

On the benchmark payloads these cut encoded size by **76.6%** (nested field
groups) and **31.9%** (keyed tabular) against the v3.x nested output.

Spec v4.0 also **removed** key folding and path expansion — folded output is
still valid TOON (dotted keys are literal keys), but no decoder re-nests it, so
`fold_keys` is now an etoon extension rather than a spec option. The upstream
rationale is in [`.out-of-scope/key-folding.md`](https://github.com/toon-format/spec/blob/main/.out-of-scope/key-folding.md):
0.00% token savings on the reference benchmarks, wire ambiguity against literal
dotted keys, and incompatibility with streaming decode.

## Sigil-prefixed keys (`@`, `$`, `#`)

Keys starting with `@`, `$`, or `#` are treated as valid identifiers — **no quoting needed**. This gives native support for:

| Sigil | Ecosystem | Examples |
|-------|-----------|----------|
| `@`   | AWS CloudWatch, Elasticsearch, Serilog, XML→JSON | `@timestamp`, `@message`, `@version` |
| `$`   | MongoDB, JSON Schema, AWS CloudFormation | `$match`, `$ref`, `$schema`, `$type` |
| `#`   | JSON-LD, Azure Resource Manager | `#comment`, `#id` |

```bash
# AWS CloudWatch Insights output
echo '[{"@timestamp":"2026-04-06T12:00:01Z","@message":"POST /api/v1/users 504","statusCode":504}]' | etoon
# [1]{@timestamp,@message,statusCode}:
#   "2026-04-06T12:00:01Z",POST /api/v1/users 504,504
```

### Token savings (5 AWS CloudWatch log entries)

**tiktoken (offline, BPE tokenizer):**

| Tokenizer (model family) | JSON | TOON | Saved |
|--------------------------|------|------|-------|
| o200k_base (GPT-4o/5/o3) | 484 | 334 | **31.0%** |
| cl100k_base (GPT-4/3.5 ≈ Claude) | 479 | 332 | **30.7%** |

**[tokencalculator.ai](https://tokencalculator.ai/) (online, estimated per-model cost):**

| Model | JSON | TOON | Saved |
|-------|------|------|-------|
| Est. Tokens | 314 | 189 | **39.8%** |
| OpenAI GPT-5.4 | $0.000785 | $0.000473 | 39.7% |
| Claude Opus 4.6 | $0.001570 | $0.000945 | 39.8% |
| Gemini 3.1 Pro | $0.000628 | $0.000378 | 39.8% |
| DeepSeek V3.2 | $0.000088 | $0.000053 | 39.8% |
| Grok 4.20 | $0.000063 | $0.000038 | 39.7% |

Savings increase with volume — 50 entries reach **35%+** (tiktoken) as the tabular header is amortized.

## Advanced options

> Intended for **programmatic use in your codebase** (Python / Rust library calls). The CLI `| etoon` pipe for LLM workflows uses defaults and does not need these.

```python
# Custom delimiter (when values contain commas) — TOON spec §11
etoon.dumps(data, delimiter="|")   # or "\t"

# Key folding: collapse {a:{b:{c:1}}} → "a.b.c: 1"
# etoon extension — removed from the spec in v4.0, so nothing re-nests it.
etoon.dumps(data, fold_keys=True)
etoon.dumps(data, fold_keys=True, flatten_depth=2)  # partial fold
```

## Limitations

- Integers > 2⁶³ are lossily coerced via f64 (works for most common big integers
  that happen to be representable; arbitrary-precision is not supported).
- `indentSize` is hardcoded to 2 spaces (TOON spec default).
- Encoder only — etoon does not decode TOON back to JSON.

## License

Apache 2.0. Test fixtures in `tests/fixtures/` come from the
[toon-format/spec](https://github.com/toon-format/spec) suite (MIT), except the
etoon-local `key-folding.json` which derives from
[toons](https://github.com/alesanfra/toons) (Apache 2.0). See
[ATTRIBUTION.md](ATTRIBUTION.md).
