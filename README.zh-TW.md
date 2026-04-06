# etoon

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

### Python
```bash
pip install etoon
```

### Rust library
```bash
cargo add etoon --no-default-features
```

### CLI binary
從 [GitHub Releases](https://github.com/coseto6125/etoon/releases) 下載，或：
```bash
cargo install etoon
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

## 進階選項

```python
# 自訂分隔符（資料含逗號時可省 token）
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
