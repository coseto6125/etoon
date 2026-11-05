# etoon

[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/coseto6125/etoon/badge)](https://scorecard.dev/viewer/?uri=github.com/coseto6125/etoon)
[![SLSA 3](https://slsa.dev/images/gh-badge-level3.svg)](https://slsa.dev)
[![VirusTotal](https://img.shields.io/badge/VirusTotal-scanned-brightgreen?logo=virustotal)](https://github.com/coseto6125/etoon/releases)
[![cargo audit](https://img.shields.io/badge/cargo_audit-passing-brightgreen?logo=rust)](https://rustsec.org)

快速的 [TOON](https://github.com/toon-format/toon) (Token-Oriented Object Notation) 編碼器，支援 Python、Rust、CLI。

**比 `toons` 最多快 7.4 倍**、**比官方 TS SDK 快 2.9–8.7 倍**，spec 套件上輸出 byte-identical，對齊 **TOON spec v4.4**。

[English](https://github.com/coseto6125/etoon/blob/main/README.md)

## 效能

各種代表性 payload 的單次編碼時間（`etoon` = Python/PyO3，best-of-7 × 2000 次）。
`✓` = 輸出與 etoon byte-identical；`✗` = 該編碼器輸出不同 — 或偏離 spec（py-rtoon
對整數值浮點輸出 `0.0`，spec 要求 `0`），或仍停在 spec v3.x、把 v4.0 的收合形式
展開成巢狀區塊。

| Payload（編碼）          | etoon   | toons           | py-rtoon        | @toon-format/toon 4.4 (TS) |
|--------------------------|---------|-----------------|-----------------|----------------------------|
| 1000 筆同構物件          | 181 µs  | 802 µs (4.4×✓)  | 846 µs (4.7×✗)  | 535 µs (3.0×✓)             |
| 深層巢狀                 | 124 µs  | 302 µs (2.4×✓)  | 723 µs (5.8×✗)  | 674 µs (5.4×✓)             |
| 1000 筆字串記錄          | 97 µs   | 714 µs (7.4×✓)  | 595 µs (6.1×✓)  | 633 µs (6.5×✓)             |
| 500 筆混合物件           | 141 µs  | 708 µs (5.0×✓)  | 605 µs (4.3×✗)  | 1235 µs (8.7×✓)            |
| 1000 筆巢狀欄位群組      | 209 µs  | 1166 µs (5.6×✗) | 1054 µs (5.0×✗) | 601 µs (2.9×✓)             |
| 1000 筆 keyed 表格列     | 116 µs  | 731 µs (6.3×✗)  | 624 µs (5.4×✗)  | 588 µs (5.1×✓)             |

**比所有其他編碼器快 2.4–8.7 倍**，且輸出 **byte-identical、符合 spec canonical**：
六個 payload 全部逐位元組一致的只有官方 TS SDK。

CLI（`… | etoon`）在此之上多了進程啟動 + pipe I/O — 適合 shell pipeline / LLM log，
但程式內呼叫請優先用 PyO3 的 `dumps`（無進程啟動、無 pipe）。Auto-detect 模式
（JSON / 混合 log / 純文字）在 100–600 KB 輸入上約 0.6–1.9 ms / 次。

### 自行測試

```bash
# 速度 + parity 對比 toons / py-rtoon / TS SDK（未安裝者自動略過）：
pip install -e '.[bench]'        # toons + py-rtoon
npm install @toon-format/toon    # 選用：TS SDK 對比
python benches/compare.py        # 可加 --iters N 調整

# Encoder core benchmark（Rust native，不含 Python/PyO3 開銷）：
cargo run --release --bin bench payload.json
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

也可以交給 `PreToolUse` hook 自動補上。`etoon hook claude` 會在每個 Bash 命令後面接
`| etoon`，不必靠模型記得規則。加到 `~/.claude/settings.json`（或專案的 `.claude/settings.json`）：

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "etoon hook claude" }] }
    ]
  }
}
```

hook 把 `CMD` 改寫成
`CMD | etoon; __etoon_rc=${PIPESTATUS[${#PIPESTATUS[@]}-2]}; (exit $__etoon_rc)`。
後綴還原 etoon 前一個 pipeline 元素的 exit status，失敗的命令仍然回報失敗。改寫前先去掉結尾換行與結尾的空白行。
`tool_input` 的其他欄位（`description`、`timeout`、`run_in_background`）會保留，rtk 有設定的欄位除外（見下文）。

改寫的限制：

- 後綴需要 **bash**（`PIPESTATUS`）。在 zsh 下狀態會讀成 0。
- `etoon` 必須在 Bash 工具的 `PATH` 上，否則輸出會遺失。
- 複合命令（`a; b`、`a && b`）只有最後一個 pipeline 經過 etoon。
- `! CMD`，或 session 先前已設 `pipefail`，會改變後綴無法還原的狀態。結尾的 `# 註解` 會把後綴一起註解掉（不會編碼）。

命中下列任一規則時，命令不接 etoon：

| # | 規則 | 原因 |
|---|------|------|
| 1 | 命令已經接了 `etoon` | 避免重複 |
| 2 | 結尾是 `&`、`;` 或 `\` | 背景工作、語法錯誤、續行 |
| 3 | 某一行最後一段 pipe 有輸出重導（`> f`、`>> f`） | 輸出已寫入檔案 |
| 4 | pipe 接上的那段命令是 shell builtin（`cd`、`export`、`source`……），包含 `&&`、`\|\|`、`;`、`&` 之後的 | pipe 會讓它跑在 subshell |
| 5 | `git diff` 或 `diff` | 需要 byte-exact 輸出 |
| 6 | 明確要 JSON（`--json`、`-o json`、`--format=json`、`{{json .}}`） | 需要 byte-exact 輸出 |
| 7 | 含 heredoc（`<<`，不含 `<<<`） | pipe 會破壞結尾定界符 |
| 8 | 含 `pipefail` | 後綴只讀一個 pipeline 元素的狀態 |

**rtk。** `PATH` 上有 [rtk](https://github.com/rtk-ai/rtk) 時，hook 先跑 `rtk hook claude`，
再在 rtk 的改寫後面接 etoon。Claude Code 每次工具呼叫只採用一份 `updatedInput`，
所以用這個 hook 取代 rtk 自己的 hook，不要兩個都掛。skip 規則看的是 rtk 改寫前的命令。
rtk 的 `updatedInput` 欄位會取代 tool input 的同名欄位，其餘欄位保留。rtk 最多執行 5 秒。

**`--pre CMD`**（也可寫成 `--pre=CMD`，最多一次）。外部改寫器在 rtk 與 etoon 之前執行：

- `CMD` 依空白切成程式與參數，不支援 shell 引號。hook 把 Bash 命令接在最後一個參數。
- 改寫器在 stdout 印出新命令並以 0 結束。結尾換行會被去掉。
- 非 0 結束、輸出為空、只有空白或非 UTF-8、無法啟動、執行超過 2 秒，都保留原命令。
- 含 NUL byte 或以 `-` 開頭的命令不經過改寫器。
- rtk 失敗或命中 skip 規則時，改寫後的命令照樣送出。

```bash
etoon hook claude --pre "my-rewriter --mode strict"
# 執行：my-rewriter --mode strict '<Bash 命令>'
```

hook 會丟棄 rtk 與改寫器的 stderr。逾時只會終止直接子程序。

**Fail open。** hook 不會因為自己的錯誤擋下命令：

| 情況 | stdout | Exit |
|------|--------|------|
| 改寫 | 一份 JSON decision | 0 |
| 不是 Bash、沒有命令、命令為空或只有空白、stdin 不是 JSON | 無 | 0 |
| rtk 失敗（任何非 0 exit、逾時、JSON 壞掉） | 無，或 `--pre` 的改寫 | 0 |
| hook 內部 panic | 無 | 0 |
| hook 參數錯誤 | 無（usage 印在 stderr） | 1 |

exit 1 時 Claude Code 照樣執行命令。`etoon hook claude --help` 印出 usage。只打 `etoon hook`（沒有 `claude`）仍然讀取名為 `hook` 的檔案。

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
- **first-row column probe** — 只看第一個元素就能排除 tabular（含 array 或空物件
  的欄位），混合陣列以 O(欄位數) 落到 list form
- **itoa 特化整數格式化**

## 相容性

對齊 **TOON spec v4.4**。通過官方
[`toon-format/spec`](https://github.com/toon-format/spec) encode fixture 套件（`v4.4.0`）
**158/160**：未過的兩條都需要非預設的 `indentSize`（etoon 固定 2 spaces）。

輸出與官方 `toon-format/toon` TypeScript SDK 4.4 **byte-identical**，只有三處不同：

| 輸入                                     | etoon                         | TS SDK 4.4                    | Spec              |
|------------------------------------------|-------------------------------|-------------------------------|-------------------|
| 0 < \|n\| < 1e-6 或 \|n\| ≥ 1e21 的數字 | 一般小數，`0.0000001`         | 指數，`1e-7`                  | §2 兩者皆可       |
| 超過 2^53、仍在 i64 或 u64 範圍內的整數 | 精確，`123456789012345678`    | 捨入，`123456789012345680`    | —                 |
| `@`、`$`、`#` 開頭的 key                 | 不加引號，`#id: 1`            | 加引號，`"#id": 1`            | §7.3 要求加引號   |

Sigil key 是 etoon 刻意的擴充（見 [Sigil 前綴 key](#sigil-前綴-key)）。spec decoder
把 `#` 開頭的行當成註解：值是 scalar 的 `#` key 會被丟掉，值是物件或陣列的 `#` key
可能讓整份文件解碼失敗。

### v4 收合形式

spec v4.0 新增兩種把巢狀壓平的形式，兩者都已實作：

```bash
# 巢狀欄位群組（§9.3）— 同構的巢狀物件變成表頭欄位
echo '[{"id":1,"customer":{"name":"Ada","country":"DK"},"total":99}]' | etoon
# [1]{id,customer{name,country},total}:
#   1,Ada,DK,99

# Keyed tabular（§9.5）— 值為同構物件的物件變成帶 key 的表格
echo '{"alpha":{"host":"a.example.com","port":8080},"beta":{"host":"b.example.com","port":9090}}' | etoon
# [2:]{host,port}:
#   alpha: a.example.com,8080
#   beta: b.example.com,9090
```

在 benchmark payload 上，相較 v3.x 的巢狀輸出分別縮小 **76.6%**（巢狀欄位群組）
與 **31.9%**（keyed tabular）。

spec v4.0 同時**移除**了 key folding 與 path expansion — 折疊後的輸出仍是合法
TOON（點號 key 就是字面 key），但沒有 decoder 會把它還原成巢狀，所以 `fold_keys`
現在是 etoon 的擴充而非 spec 選項。上游理由見
[`.out-of-scope/key-folding.md`](https://github.com/toon-format/spec/blob/main/.out-of-scope/key-folding.md)：
在參考 benchmark 上 token 節省 0.00%、與字面點號 key 產生線上歧義、且與 streaming
decode 不相容。

## Sigil 前綴 key（`@`、`$`、`#`）

以 `@`、`$`、`#` 開頭的 key 視為合法 identifier — **不需要加引號**。原生支援以下生態系：

> **注意：** 這偏離 spec §7.3（該條要求這類 key 加引號）。輸出是給 LLM 讀的。TOON spec
> decoder 把 `#` 開頭的行當成註解（§5.1）：值是 scalar 的 `#` key 會被丟掉，值是物件或
> 陣列的 `#` key 可能讓整份文件解碼失敗。

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

**tiktoken（離線 BPE tokenizer）：**

| Tokenizer（模型系列）             | JSON | TOON | 節省       |
|-----------------------------------|------|------|-----------|
| o200k_base (GPT-4o/5/o3)         | 484  | 334  | **31.0%** |
| cl100k_base (GPT-4/3.5 ≈ Claude) | 479  | 332  | **30.7%** |

**[tokencalculator.ai](https://tokencalculator.ai/)（線上，各模型費用估算）：**

| 模型               | JSON       | TOON       | 節省       |
|--------------------|-----------|-----------|-----------|
| Est. Tokens        | 314       | 189       | **39.8%** |
| OpenAI GPT-5.4     | $0.000785 | $0.000473 | 39.7%     |
| Claude Opus 4.6    | $0.001570 | $0.000945 | 39.8%     |
| Gemini 3.1 Pro     | $0.000628 | $0.000378 | 39.8%     |
| DeepSeek V3.2      | $0.000088 | $0.000053 | 39.8%     |
| Grok 4.20          | $0.000063 | $0.000038 | 39.7%     |

量越大節省越多 — 50 筆達 **35%+**（tiktoken），因為 tabular header 開銷被攤薄。

## 進階選項

> 適用於 **codebase 內的程式呼叫**（Python / Rust library）。CLI 的 `| etoon` pipe 使用預設值，不需要設定這些。

```python
# 自訂分隔符（資料含逗號時使用）— TOON spec §11
etoon.dumps(data, delimiter="|")   # 或 "\t"

# Key folding：壓扁 {a:{b:{c:1}}} → "a.b.c: 1"
# etoon 擴充 — spec v4.0 已移除，沒有 decoder 會還原它。
etoon.dumps(data, fold_keys=True)
etoon.dumps(data, fold_keys=True, flatten_depth=2)  # 部分 fold
```

## 限制

- 超過 2⁶³ 的整數會被降為 f64（多數能整數表示的 1e20 等仍可來回，
  但任意精度不支援）。
- `indentSize` 固定 2 spaces（TOON spec 預設）。
- 只做編碼 — etoon 不提供 TOON → JSON 的解碼。

## 授權

Apache 2.0。`tests/fixtures/` 測試檔案來自
[toon-format/spec](https://github.com/toon-format/spec) 官方套件（MIT），
唯獨 etoon 自有的 `key-folding.json` 衍生自
[toons](https://github.com/alesanfra/toons)（Apache 2.0）。詳見
[ATTRIBUTION.md](ATTRIBUTION.md)。
