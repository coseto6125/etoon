# etoon

快速的 [TOON](https://github.com/toon-format/toon) (Token-Oriented Object Notation) 編碼器，支援 Python、Rust、CLI。

**比 `toons` 快 8 倍**、**比官方 TS SDK 快 2.7 倍**，輸出 byte-identical。

[English](./README.md)

## 效能

50 筆 neptune payload 實測（7480 bytes JSON → 4012 bytes TOON）：

| 編碼器                | 時間     | 相對倍數 |
|-----------------------|----------|---------|
| **etoon (Python)**    | 13.0 μs  | **1.00×** |
| @toon-format/toon (TS)| 35.6 μs  | 2.73×    |
| py-rtoon              | 85.9 μs  | 6.59×    |
| toons                 | 106.4 μs | 8.17×    |

**CLI 透過 stdin pipe**（Claude / Bash 工作流）：

| CLI           | 每次延遲  | 相對倍數   |
|---------------|----------|-----------|
| **etoon**     | 0.57 ms  | **1.00×** |
| 官方 toon     | 50.7 ms  | 慢 89×    |

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

## 限制

- 超過 2⁶³ 的整數會被降為 f64（多數能整數表示的 1e20 等仍可來回，
  但任意精度不支援）。
- `indent` 固定 2 spaces（TOON spec 預設）。
- TOON 進階功能未支援：`--fold-keys`、`--delimiter tab/pipe`。

## 授權

Apache 2.0。`tests/fixtures/` 測試檔案來自
[toons](https://github.com/alesanfra/toons) 專案（Apache 2.0）。
