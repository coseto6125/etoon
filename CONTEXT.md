# etoon — domain glossary

Terms used across code, fixtures, and docs. Architecture vocabulary follows
the deep-module terms: module, interface, implementation, depth, seam,
adapter, leverage, locality.

## Encoder options

The seven knobs every encoder adapter exposes. `Config` (`src/toon.rs`) holds
the single source of truth for their defaults; each language adapter mirrors
them once — Python truth is the `dumps()` keyword signature, Rust truth is
`Config::default`. The raw binding called with zero kwargs is the only path
that exercises `Config::default`; `tests/test_defaults_parity.py` pins both
definitions together through observable behavior. When an option is added:
add it to `Config` + one adapter mirror + `OPTION_NAMES`, then extend the
parity probes.

| term | meaning |
|---|---|
| delimiter | byte separating array/tabular values: `,` `	` or `\|` |
| key folding (`fold_keys`) | collapse single-key object chains into dot-notation keys; etoon extension, safe mode |
| flatten depth | max chain length when folding; None = unlimited |
| bare empty arrays (`empty_array_bare`) | emit `[]` / `key: []` instead of legacy `[0]:` length markers |
| control escaping (`escape_controls`) | emit U+0000–U+001F as lowercase `\uXXXX` |
| max depth (`max_depth`) | reject deeper input pre-parse; guards stack overflow; 0 disables |
| max input bytes (`max_input_bytes`) | reject larger raw input; bounds memory; 0 disables |

## Output forms

- **tabular form**: array-of-objects emitted as `[N]{fields}:` header plus rows
- **keyed table** (§9.5): object-of-objects emitted as `[N:]{fields}:` plus keyed rows
- **keyless header**: fields-bearing header without a key; valid only at document root (§6)
- **list form**: `- `-prefixed items when tabular detection declines

## Layers

Python caller → `etoon.dumps` (facade, Python truth) → `_etoon.dumps_bytes`
(PyO3 adapter, options seam) → `toon::encode_with` (core module).
The CLI binary (`src/bin/etoon.rs`) is a second adapter that uses pure Rust
defaults via `toon::encode`.
