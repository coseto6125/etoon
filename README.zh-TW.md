# etoon

[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/coseto6125/etoon/badge)](https://scorecard.dev/viewer/?uri=github.com/coseto6125/etoon)
[![SLSA 3](https://slsa.dev/images/gh-badge-level3.svg)](https://slsa.dev)
[![VirusTotal](https://img.shields.io/badge/VirusTotal-scanned-brightgreen?logo=virustotal)](https://github.com/coseto6125/etoon/releases)
[![cargo audit](https://img.shields.io/badge/cargo_audit-passing-brightgreen?logo=rust)](https://rustsec.org)

快速的 [TOON](https://github.com/toon-format/toon) (Token-Oriented Object Notation) 編碼器，支援 Python、Rust、CLI。

**比 `toons` 快 8 倍**、**比官方 TS SDK 快 2.7 倍**，輸出 byte-identical。

[English](https://github.com/coseto6125/etoon/blob/main/README.md)

## 效能

50 筆 payload 實測（7480 bytes JSON → 4012 bytes TOON）：

| 編碼器                     | 時間     | 相對倍數   |
|----------------------------|----------|-----------|
| **etoon (Rust native)**    | 12.1 μs  | **1.00×** |
| **etoon (Python, PyO3)**   | 15.4 μs  | 1.27×     |
| @toon-format/toon (TS SDK) | 35.6 μs  | 2.94×     |
| py-rtoon                   | 85.9 μs  | 7.10×     |
| toons                      | 106.4 μs | 8.79×     |

**CLI 透過 stdin pipe**（Claude / Bash 工作流）：

| CLI           | 每次延遲  | 相對倍數    |
|---------------|----------|------------|
| **etoon**     | 0.43 ms  | **1.00×**  |
| 官方 toon     | 50.7 ms  | 慢 118×    |

**Auto-detect 模式**（v0.2.0+）— 自動辨識 JSON、混合 log、純文字：

| 輸入                            | 大小   | 每次延遲  |
|---------------------------------|--------|----------|
| 純 JSON（1000 objects）         | 120KB  | 0.73 ms  |
| 混合 log（5K JSON + 5K 文字行） | 600KB  | 1.93 ms  |
| 純文字 pass-through             | 300KB  | 0.56 ms  |

### 自行測試

```bash
# Encoder core benchmark（Rust native，不含 I/O）
cargo run --release --bin bench payload.json

# CLI stdin pipe benchmark
# 產生測試資料
python3 -c "
import json
data = [{'id': i, 'name': f'item_{i}', 'price': i*1.5, 'tags': ['a','b','c']} for i in range(1000)]
print(json.dumps(data))
" > /tmp/bench.json

# 計時（200 次取平均）
start=$(date +%s%N)
for i in $(seq 1 200); do etoon < /tmp/bench.json > /dev/null; done
end=$(date +%s%N)
echo "$(echo "scale=2; ($end - $start) / 200000000" | bc)ms avg"
```

## 安裝

### CLI binary（LLM 工作流推薦）

**預編譯版 — 不需要 Rust：**

從 [GitHub Releases](https://github.com/coseto6125/etoon/releases) 下載（Linux/macOS/Windows，x86_64/aarch64）：

<details>
<summary><b>Linux</b></summary>

```bash
# x86_64
curl -L https://github.com/coseto6125/etoon/releases/latest/download/etoon-linux-x86_64 -o etoon

# Apple Silicon / ARM 伺服器 (aarch64)
curl -L https://github.com/coseto6125/etoon/releases/latest/download/etoon-linux-aarch64 -o etoon

chmod +x etoon
sudo mv etoon /usr/local/bin/   # 或 ~/.local/bin/
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

# 移動到 PATH 目錄，例如：
Move-Item etoon.exe "$env:USERPROFILE\.local\bin\etoon.exe"
```
</details>

<details>
<summary><b>驗證下載（可選）</b></summary>

每個 release 都附帶 SHA256 checksum、[SLSA provenance](https://slsa.dev) 證明、和 [VirusTotal](https://www.virustotal.com) 掃描報告。

```bash
# 1. 驗證 checksum
curl -L https://github.com/coseto6125/etoon/releases/latest/download/SHA256SUMS.txt -o SHA256SUMS.txt
sha256sum -c SHA256SUMS.txt --ignore-missing

# 2. 驗證 SLSA provenance（需要 gh CLI）
gh attestation verify etoon-linux-x86_64 --repo coseto6125/etoon

# 3. VirusTotal — 掃描報告連結在 release notes 裡
```

macOS 未簽名 binary 提示：`xattr -d com.apple.quarantine etoon` 可繞過 Gatekeeper。
</details>

**從原始碼編譯（需要 Rust toolchain）：**

```bash
cargo install etoon
```

### Python library

```bash
pip install etoon
```

> 這會安裝 Python binding（`etoon.dumps()`），**不包含** CLI binary。CLI 請用上面的方式安裝。

### Rust library

```bash
cargo add etoon --no-default-features
```

## 使用

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
aws logs tail /ecs/my-service | etoon        # 混合 log：JSON 區塊 → TOON，文字 → pass-through
etoon --strict < data.json                   # 非 JSON 時報錯（舊行為）
```

### LLM Agent — 設定指南

在 shell 指令末尾加 `| etoon`，讓 AI 讀 TOON 而非原始 JSON — **省 40-60% token**，無損，自動辨識輸入格式。

```bash
gh pr list --json number,title,author | etoon
aws logs tail /ecs/my-service | etoon
kubectl get pods -o json | etoon
```

**規則**：
- `| etoon` 放在 pipeline **最後** — `jq` / `grep` / `awk` 在前，絕不在後
- 自動辨識 JSON / 混合 log / 純文字 — 任何指令都可安全附加
- 僅跳過：byte-exact diff、單值查詢（1-2 行）

---

#### Claude Code

加到專案根目錄的 `CLAUDE.md`（或 `.claude/CLAUDE.md`）：

````markdown
## Bash Output

Append `| etoon` as the final pipe segment of any Bash command whose output
reaches the conversation. etoon auto-detects JSON, mixed log, and plain text.
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### Cursor

加到 `.cursor/rules/etoon.mdc`：

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

加到專案根目錄的 `AGENTS.md`：

````markdown
## Bash Output

Append `| etoon` as the final pipe segment of any shell command.
etoon auto-detects JSON, mixed log, and plain text (non-JSON passes through unchanged).
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### Windsurf / 其他 AI 編輯器

加到 `.windsurfrules`（Windsurf）或對應的規則檔：

````markdown
When executing shell commands, always append `| etoon` at the end of the pipeline.
etoon auto-detects input format: JSON → compact TOON, mixed log → JSON blocks encoded + text pass-through, plain text → unchanged.
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### Gemini CLI

加到專案根目錄的 `GEMINI.md`：

````markdown
## Bash Output

Append `| etoon` as the final pipe segment of any shell command.
etoon auto-detects JSON, mixed log, and plain text (non-JSON passes through unchanged).
Skip only for byte-exact diff or single-value lookups (1-2 lines).
````

#### ChatGPT / Custom GPTs

加到 system prompt 或自訂指令：

```
When generating shell commands for the user, append `| etoon` as the last pipe segment.
etoon converts JSON to TOON (40-60% fewer tokens). Non-JSON passes through unchanged.
```

### Rust
```rust
let json_bytes = serde_json::to_vec(&my_data)?;
let toon = etoon::toon::encode(&json_bytes)?;
```

## 架構

```
Python dict → orjson.dumps → JSON bytes → sonic-rs (SIMD parse) → walk → TOON string
```

核心優化：
- **sonic-rs SIMD JSON parser**（比 serde_json 快 ~7×）
- **orjson bridge** — 只跨一次邊界（vs PyO3-based 方案需多次）
- **uniform-order table fast path** — 50 筆 row 省 300 次 key lookup
- **itoa 特化整數格式化**

## 相容性

輸出與 Python 套件 `toons`（Apache 2.0）和官方 `toon-format/toon`
TypeScript SDK **byte-identical**。通過 **111/111** TOON spec fixtures
涵蓋 primitives、objects、arrays（primitive/tabular/nested/bulleted）、
whitespace。

## Sigil 前綴 key（`@`、`$`、`#`）

以 `@`、`$`、`#` 開頭的 key 視為合法 identifier — **不需要加引號**。原生支援以下生態系：

| 前綴  | 生態系                                               | 常見 key                                 |
|-------|------------------------------------------------------|------------------------------------------|
| `@`   | AWS CloudWatch, Elasticsearch, Serilog, XML→JSON     | `@timestamp`, `@message`, `@version`     |
| `$`   | MongoDB, JSON Schema, AWS CloudFormation              | `$match`, `$ref`, `$schema`, `$type`     |
| `#`   | JSON-LD, Azure Resource Manager                       | `#comment`, `#id`                        |

```bash
# AWS CloudWatch Insights 輸出
echo '[{"@timestamp":"2026-04-06T12:00:01Z","@message":"POST /api/v1/users 504","statusCode":504}]' | etoon
# [1]{@timestamp,@message,statusCode}:
#   "2026-04-06T12:00:01Z",POST /api/v1/users 504,504
```

### Token 節省實測（5 筆 AWS CloudWatch log）

| Tokenizer（模型系列）                | JSON | TOON | 節省       |
|--------------------------------------|------|------|-----------|
| o200k_base (GPT-4o/5/o3)            | 484  | 334  | **31.0%** |
| cl100k_base (GPT-4/3.5 ≈ Claude)    | 479  | 332  | **30.7%** |
| tokencalculator.ai（全模型）          | 314  | 189  | **39.8%** |

量越大節省越多 — 50 筆達 **35%+**（tiktoken），因為 tabular header 開銷被攤薄。

## 進階選項

> 這些是 [TOON spec](https://github.com/toon-format/toon) 提供的可選參數，適用於 **codebase 內的程式呼叫**（Python / Rust library）。CLI 的 `| etoon` pipe 使用預設值，不需要設定這些。

```python
# 自訂分隔符（資料含逗號時使用）
etoon.dumps(data, delimiter="|")   # 或 "\t"

# Key folding：壓扁 {a:{b:{c:1}}} → "a.b.c: 1"
etoon.dumps(data, fold_keys=True)
etoon.dumps(data, fold_keys=True, flatten_depth=2)  # 部分 fold
```

## 限制

- 超過 2⁶³ 的整數會被降為 f64（多數能整數表示的 1e20 等仍可來回，
  但任意精度不支援）。
- `indent` 固定 2 spaces（TOON spec 預設）。

## 授權

Apache 2.0。`tests/fixtures/` 測試檔案來自
[toons](https://github.com/alesanfra/toons) 專案（Apache 2.0）。
