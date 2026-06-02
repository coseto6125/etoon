//! TOON encoder core (sonic-rs backend).
//!
//! Input: JSON bytes (from orjson.dumps on Python side).
//! Output: TOON string, matching TOON spec v3.1.
//!
//! Delimiter is monomorphized via const generics (`DELIM: u8`) so the
//! byte-match inner loops fold away when emitting default-comma output.

use sonic_rs::{Array, JsonContainerTrait, JsonType, JsonValueTrait, Object, Value};
use std::fmt::Write as _;

/// Encoder configuration matching TOON spec v3.1 options.
#[derive(Clone, Copy)]
pub struct Config {
    /// Delimiter between array/tabular values. Must be `,`, `\t`, or `|`.
    pub delimiter: u8,
    /// If true, fold single-key object chains into dot-notation keys (safe mode).
    pub key_folding: bool,
    /// Max fold depth (segments). None = unlimited. 0 disables folding.
    pub flatten_depth: Option<usize>,
    /// If true (v3.1), emit empty arrays as canonical `[]` / `key: []`
    /// instead of the legacy `[0]:` / `key[0]:` length-marker form.
    pub empty_array_bare: bool,
    /// If true (v3.1), escape control chars U+0000–U+001F (except the named
    /// `\n` `\r` `\t`) as `\uXXXX` with lowercase hex.
    pub escape_controls: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            delimiter: b',',
            key_folding: false,
            flatten_depth: None,
            empty_array_bare: true,
            escape_controls: true,
        }
    }
}

pub fn encode(json_bytes: &[u8]) -> Result<String, String> {
    encode_with(json_bytes, &Config::default())
}

pub fn encode_with(json_bytes: &[u8], cfg: &Config) -> Result<String, String> {
    let value: Value =
        sonic_rs::from_slice(json_bytes).map_err(|e| format!("JSON parse error: {}", e))?;
    let mut out = String::with_capacity(json_bytes.len());
    match cfg.delimiter {
        b',' => write_root::<b','>(&value, cfg, &mut out),
        b'\t' => write_root::<b'\t'>(&value, cfg, &mut out),
        b'|' => write_root::<b'|'>(&value, cfg, &mut out),
        _ => return Err("delimiter must be ',', '\\t', or '|'".to_string()),
    }
    Ok(out)
}

fn write_root<const DELIM: u8>(v: &Value, cfg: &Config, out: &mut String) {
    match v.get_type() {
        JsonType::Object => {
            let m = v.as_object().unwrap();
            if !m.is_empty() {
                // Folding is attempted at the top-level object; nested object
                // bodies re-apply it via write_value_after_key (spec §13.4).
                write_object_body::<DELIM>(m, 0, cfg, cfg.key_folding, out);
            }
        }
        JsonType::Array => {
            let arr = v.as_array().unwrap();
            // Root empty array: v3.1 canonical bare `[]` (no leading colon).
            if arr.is_empty() && cfg.empty_array_bare {
                out.push_str("[]");
            } else {
                write_array_suffix::<DELIM>(arr, 0, cfg, out);
            }
        }
        _ => write_scalar::<DELIM>(v, cfg, out),
    }
}

fn write_object_body<const DELIM: u8>(
    m: &Object,
    indent: usize,
    cfg: &Config,
    allow_fold: bool,
    out: &mut String,
) {
    let mut first = true;
    for (k, v) in m.iter() {
        if !first {
            out.push('\n');
        }
        first = false;
        write_indent(indent, out);

        if allow_fold {
            if let Some((joined, final_v)) = try_fold(k, v, cfg, m) {
                write_key(&joined, cfg, out);
                write_value_after_key::<DELIM>(final_v, indent, cfg, out);
                continue;
            }
        }

        write_key(k, cfg, out);
        write_value_after_key::<DELIM>(v, indent, cfg, out);
    }
}

fn try_fold<'a>(k: &'a str, v: &'a Value, cfg: &Config, m: &Object) -> Option<(String, &'a Value)> {
    let max_depth = cfg.flatten_depth.unwrap_or(usize::MAX);
    if max_depth < 2 {
        return None;
    }

    // Key segments must match TOON identifier pattern (safe mode).
    if key_needs_quoting(k) {
        return None;
    }

    let mut cur_v = v;
    let mut path: Vec<&'a str> = vec![k];

    loop {
        if path.len() >= max_depth {
            break;
        }
        let obj = match cur_v.get_type() {
            JsonType::Object => cur_v.as_object().unwrap(),
            _ => break,
        };
        if obj.len() != 1 {
            break;
        }
        let (nk, nv) = obj.iter().next().unwrap();
        if key_needs_quoting(nk) {
            break;
        }
        path.push(nk);
        cur_v = nv;
    }

    if path.len() < 2 {
        return None;
    }

    let joined: String = path.join(".");

    if m.get(&joined).is_some() {
        return None;
    }

    Some((joined, cur_v))
}

fn write_value_after_key<const DELIM: u8>(
    v: &Value,
    key_indent: usize,
    cfg: &Config,
    out: &mut String,
) {
    match v.get_type() {
        JsonType::Object => {
            let child = v.as_object().unwrap();
            if child.is_empty() {
                out.push(':');
            } else {
                out.push_str(":\n");
                // Folding restarts only at a branch point (multi-key object).
                // A single-key body is part of a chain whose fold decision was
                // already made by the parent's try_fold — re-folding it would
                // wrongly bypass collision/flattenDepth stops (spec §13.4).
                let allow = cfg.key_folding && child.len() > 1;
                write_object_body::<DELIM>(child, key_indent + 1, cfg, allow, out);
            }
        }
        JsonType::Array => {
            let arr = v.as_array().unwrap();
            // Object value: v3.1 canonical `key: []`; legacy `key[0]:` otherwise.
            if arr.is_empty() && cfg.empty_array_bare {
                out.push_str(": []");
            } else {
                write_array_suffix::<DELIM>(arr, key_indent, cfg, out);
            }
        }
        _ => {
            out.push_str(": ");
            write_scalar::<DELIM>(v, cfg, out);
        }
    }
}

/// Emit the legacy empty-array header `[0<delim?>]:` at the current position.
/// Used in list-item context, where v3.1 keeps this form (SPEC §9.2).
fn write_empty_array_legacy<const DELIM: u8>(out: &mut String) {
    out.push_str("[0");
    if DELIM != b',' {
        out.push(DELIM as char);
    }
    out.push_str("]:");
}

fn write_array_suffix<const DELIM: u8>(arr: &Array, indent: usize, cfg: &Config, out: &mut String) {
    let _ = cfg;
    if arr.is_empty() {
        write_empty_array_legacy::<DELIM>(out);
        return;
    }

    write!(out, "[{}", arr.len()).unwrap();
    if DELIM != b',' {
        out.push(DELIM as char);
    }
    out.push(']');

    if arr.iter().all(is_scalar) {
        out.push_str(": ");
        let mut first = true;
        for v in arr.iter() {
            if !first {
                out.push(DELIM as char);
            }
            first = false;
            write_scalar::<DELIM>(v, cfg, out);
        }
        return;
    }

    if let Some((keys, uniform_order)) = table_keys(arr) {
        out.push('{');
        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                out.push(DELIM as char);
            }
            write_key(k, cfg, out);
        }
        out.push_str("}:");
        if uniform_order {
            for item in arr.iter() {
                let m = item.as_object().unwrap();
                out.push('\n');
                write_indent(indent + 1, out);
                let mut first = true;
                for (_, v) in m.iter() {
                    if !first {
                        out.push(DELIM as char);
                    }
                    first = false;
                    write_scalar::<DELIM>(v, cfg, out);
                }
            }
        } else {
            for item in arr.iter() {
                let m = item.as_object().unwrap();
                out.push('\n');
                write_indent(indent + 1, out);
                let mut first = true;
                for k in &keys {
                    if !first {
                        out.push(DELIM as char);
                    }
                    first = false;
                    write_scalar::<DELIM>(m.get(k).unwrap(), cfg, out);
                }
            }
        }
        return;
    }

    out.push(':');
    for item in arr.iter() {
        out.push('\n');
        write_indent(indent + 1, out);
        out.push('-');
        write_list_item::<DELIM>(item, indent + 1, cfg, out);
    }
}

fn write_list_item<const DELIM: u8>(v: &Value, l: usize, cfg: &Config, out: &mut String) {
    match v.get_type() {
        JsonType::Object => {
            let m = v.as_object().unwrap();
            if !m.is_empty() {
                out.push(' ');
                write_list_item_object::<DELIM>(m, l, cfg, out);
            }
        }
        JsonType::Array => {
            out.push(' ');
            write_array_suffix::<DELIM>(v.as_array().unwrap(), l, cfg, out);
        }
        _ => {
            out.push(' ');
            write_scalar::<DELIM>(v, cfg, out);
        }
    }
}

fn write_list_item_object<const DELIM: u8>(m: &Object, l: usize, cfg: &Config, out: &mut String) {
    let mut first = true;
    for (k, v) in m.iter() {
        if !first {
            out.push('\n');
            write_indent(l + 1, out);
        }
        first = false;
        write_key(k, cfg, out);
        write_value_after_key::<DELIM>(v, l + 1, cfg, out);
    }
}

// ==================== Helpers ====================

const INDENTS: [&str; 9] = [
    "",
    "  ",
    "    ",
    "      ",
    "        ",
    "          ",
    "            ",
    "              ",
    "                ",
];

#[inline]
fn write_indent(level: usize, out: &mut String) {
    if level < INDENTS.len() {
        out.push_str(INDENTS[level]);
    } else {
        for _ in 0..(level * 2) {
            out.push(' ');
        }
    }
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v.get_type(), JsonType::Object | JsonType::Array)
}

fn table_keys<'a>(arr: &'a Array) -> Option<(Vec<&'a str>, bool)> {
    let first_v = arr.iter().next()?;
    let first = first_v.as_object()?;
    if first.is_empty() {
        return None;
    }
    if !first.iter().all(|(_, v)| is_scalar(v)) {
        return None;
    }
    let keys: Vec<&'a str> = first.iter().map(|(k, _)| k).collect();
    let mut uniform_order = true;

    for item in arr.iter().skip(1) {
        let m = item.as_object()?;
        if m.len() != keys.len() {
            return None;
        }
        let mut row_iter = m.iter();
        for k in &keys {
            let (ik, iv) = row_iter.next()?;
            if !is_scalar(iv) {
                return None;
            }
            if ik != *k {
                uniform_order = false;
            }
        }
        if !uniform_order {
            for k in &keys {
                match m.get(k) {
                    Some(v) if is_scalar(v) => {}
                    _ => return None,
                }
            }
        }
    }
    Some((keys, uniform_order))
}

// ==================== Scalar ====================

#[inline]
fn write_scalar<const DELIM: u8>(v: &Value, cfg: &Config, out: &mut String) {
    match v.get_type() {
        JsonType::Null => out.push_str("null"),
        JsonType::Boolean => out.push_str(if v.as_bool().unwrap() {
            "true"
        } else {
            "false"
        }),
        JsonType::Number => write_number(v, out),
        JsonType::String => write_string_value::<DELIM>(v.as_str().unwrap(), cfg, out),
        _ => unreachable!("write_scalar on non-scalar"),
    }
}

fn write_number(v: &Value, out: &mut String) {
    if let Some(i) = v.as_i64() {
        let mut buf = itoa::Buffer::new();
        out.push_str(buf.format(i));
        return;
    }
    if let Some(u) = v.as_u64() {
        let mut buf = itoa::Buffer::new();
        out.push_str(buf.format(u));
        return;
    }
    // Non-integer or beyond u64: format once via write_float. (The old code
    // also called v.to_string() first just to probe for a decimal point,
    // formatting floats twice — dropping that probe is ~3x faster here.)
    if let Some(f) = v.as_f64() {
        write_float(f, out);
    } else {
        out.push_str("null");
    }
}

fn write_float(f: f64, out: &mut String) {
    if !f.is_finite() {
        out.push_str("null");
        return;
    }
    if f == 0.0 {
        out.push('0');
        return;
    }
    // Integer-valued float in i64 range: itoa is faster than float formatting.
    if f.fract() == 0.0 && f.abs() < 1e16 {
        let mut buf = itoa::Buffer::new();
        out.push_str(buf.format(f as i64));
        return;
    }
    // std Display gives spec-canonical decimals (expanded, no trailing zeros).
    write!(out, "{}", f).unwrap();
}

// ==================== String ====================

#[inline]
fn write_string_value<const DELIM: u8>(s: &str, cfg: &Config, out: &mut String) {
    if value_needs_quoting::<DELIM>(s, cfg.escape_controls) {
        write_quoted(s, cfg.escape_controls, out);
    } else {
        out.push_str(s);
    }
}

fn write_key(k: &str, cfg: &Config, out: &mut String) {
    if key_needs_quoting(k) {
        write_quoted(k, cfg.escape_controls, out);
    } else {
        out.push_str(k);
    }
}

/// Keys must match TOON identifier pattern: `[@$#a-zA-Z_][a-zA-Z0-9_.]*`.
/// Sigil prefixes `@`, `$`, `#` are allowed for ecosystem compatibility:
/// - `@` : AWS CloudWatch, Elasticsearch, Serilog, XML→JSON
/// - `$` : MongoDB, JSON Schema, AWS CloudFormation
/// - `#` : JSON-LD, Azure Resource Manager
#[inline]
fn key_needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let bytes = s.as_bytes();
    let start = match bytes[0] {
        b'@' | b'$' | b'#' => {
            if bytes.len() < 2 {
                return true; // bare sigil needs quoting
            }
            1
        }
        _ => 0,
    };
    let first = bytes[start];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return true;
    }
    for &b in &bytes[start + 1..] {
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
            return true;
        }
    }
    false
}

#[inline]
fn value_needs_quoting<const DELIM: u8>(s: &str, escape_controls: bool) -> bool {
    if s.is_empty() {
        return true;
    }
    let bytes = s.as_bytes();
    match bytes[0] {
        b'-' | b'[' | b'{' | b'"' | b'#' | b' ' | b'\t' => return true,
        _ => {}
    }
    match bytes[bytes.len() - 1] {
        b' ' | b'\t' => return true,
        _ => {}
    }
    // DELIM is a compile-time constant, so this match collapses into the
    // single match arm below when DELIM is in {',', '\t'} (already included),
    // and stays as a separate branch only for DELIM = '|'.
    for &b in bytes {
        match b {
            b':' | b'\n' | b'\r' | b'\t' | b'"' | b'\\' => return true,
            // Other U+0000–U+001F controls force quoting so write_quoted can
            // emit `\u00XX` (TOON spec v3.1); only when the option is on.
            _ if escape_controls && b < 0x20 => return true,
            _ if b == DELIM => return true,
            _ => {}
        }
    }
    if matches!(s, "true" | "false" | "null") {
        return true;
    }
    looks_like_number(bytes)
}

fn looks_like_number(bytes: &[u8]) -> bool {
    let mut i = 0;
    if bytes[0] == b'-' {
        i = 1;
        if i == bytes.len() {
            return false;
        }
    }
    let mut has_digit = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        has_digit = true;
        i += 1;
    }
    if !has_digit {
        return false;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let mut has_frac = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            has_frac = true;
            i += 1;
        }
        if !has_frac {
            return false;
        }
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let mut has_exp_digit = false;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            has_exp_digit = true;
            i += 1;
        }
        if !has_exp_digit {
            return false;
        }
    }
    i == bytes.len()
}

/// Lowercase hex digit for nibble `n` (0–15).
#[inline]
fn hex_lower(n: u8) -> u8 {
    match n {
        0..=9 => b'0' + n,
        _ => b'a' + (n - 10),
    }
}

fn write_quoted(s: &str, escape_controls: bool, out: &mut String) {
    out.push('"');
    let bytes = s.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        // Named escapes always apply; other U+0000–U+001F controls become
        // `\u00XX` only when escape_controls is on (TOON spec v3.1).
        let named = matches!(b, b'\\' | b'"' | b'\n' | b'\r' | b'\t');
        let other_control = escape_controls && b < 0x20;
        if named || other_control {
            if start < i {
                out.push_str(unsafe { std::str::from_utf8_unchecked(&bytes[start..i]) });
            }
            match b {
                b'\\' => out.push_str("\\\\"),
                b'"' => out.push_str("\\\""),
                b'\n' => out.push_str("\\n"),
                b'\r' => out.push_str("\\r"),
                b'\t' => out.push_str("\\t"),
                _ => {
                    // \u00XX, lowercase hex (b < 0x20 so high nibble is 0 or 1)
                    out.push_str("\\u00");
                    out.push(hex_lower(b >> 4) as char);
                    out.push(hex_lower(b & 0x0f) as char);
                }
            }
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push_str(unsafe { std::str::from_utf8_unchecked(&bytes[start..]) });
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::{encode, encode_with, Config};

    fn enc(json: &str) -> String {
        encode(json.as_bytes()).unwrap()
    }

    fn enc_with(json: &str, cfg: &Config) -> String {
        encode_with(json.as_bytes(), cfg).unwrap()
    }

    // ── Number formatting (JSON → Rust path; no Python repr to lean on) ──
    // Pin the decimal canonicalization the to_string-probe removal relies on:
    // std Display must expand small exponents and drop integer-valued `.0`.

    #[test]
    fn test_write_number_small_exponent_expands_to_decimal() {
        assert_eq!(enc(r#"{"n":1e-6}"#), "n: 0.000001");
        assert_eq!(enc(r#"{"n":1e-7}"#), "n: 0.0000001");
    }

    #[test]
    fn test_write_number_integer_valued_float_drops_fraction() {
        assert_eq!(enc(r#"{"n":100.0}"#), "n: 100");
        assert_eq!(enc(r#"{"n":-2.0}"#), "n: -2");
    }

    #[test]
    fn test_write_number_decimal_full_precision_preserved() {
        assert_eq!(enc(r#"{"n":3.14}"#), "n: 3.14");
        assert_eq!(enc(r#"{"n":0.3333333333333333}"#), "n: 0.3333333333333333");
        assert_eq!(enc(r#"{"n":1234567.89}"#), "n: 1234567.89");
    }

    #[test]
    fn test_write_number_large_magnitude_float_expands_no_exponent() {
        assert_eq!(enc(r#"{"n":1e21}"#), "n: 1000000000000000000000");
    }

    #[test]
    fn test_write_number_i64_and_u64_fast_paths() {
        assert_eq!(enc(r#"{"n":42}"#), "n: 42");
        assert_eq!(
            enc(r#"{"n":-9223372036854775808}"#),
            "n: -9223372036854775808"
        );
        assert_eq!(
            enc(r#"{"n":18446744073709551615}"#),
            "n: 18446744073709551615"
        );
    }

    #[test]
    fn test_write_number_beyond_u64_keeps_expanded_form() {
        // sonic-rs parses this through f64 (precision lost at parse time), but
        // the output must stay an expanded integer string, not an exponent.
        assert_eq!(enc(r#"{"n":1e30}"#), "n: 1000000000000000000000000000000");
    }

    // ── Empty arrays (spec v3.1 canonical, default on) ──

    #[test]
    fn test_empty_array_root_is_bare_brackets() {
        assert_eq!(enc("[]"), "[]");
    }

    #[test]
    fn test_empty_array_object_field_is_key_bracket() {
        assert_eq!(enc(r#"{"a":[]}"#), "a: []");
        assert_eq!(enc(r#"{"x":{"a":[]}}"#), "x:\n  a: []");
    }

    #[test]
    fn test_empty_array_as_array_element_keeps_legacy_header() {
        // SPEC §9.2: a bare array element that is itself empty stays `- [0]:`.
        assert_eq!(enc(r#"{"pairs":[[],[]]}"#), "pairs[2]:\n  - [0]:\n  - [0]:");
    }

    #[test]
    fn test_empty_array_legacy_form_when_option_off() {
        let cfg = Config {
            empty_array_bare: false,
            ..Config::default()
        };
        assert_eq!(enc_with("[]", &cfg), "[0]:");
        assert_eq!(enc_with(r#"{"a":[]}"#, &cfg), "a[0]:");
    }

    // ── Control-character escaping (spec v3.1, default on) ──

    // Control chars can only enter via JSON `\uXXXX` escapes — strict JSON
    // (sonic-rs) rejects raw control bytes in string literals. This mirrors the
    // Python path: orjson escapes them before they reach the Rust encoder.
    #[test]
    fn test_escape_controls_emits_lowercase_u_escape() {
        // Control chars enter only via JSON \uXXXX escapes; strict JSON
        // (sonic-rs) rejects raw control bytes. Mirrors the Python path where
        // orjson escapes them before they reach the Rust encoder.
        assert_eq!(enc("{\"s\":\"a\\u001fb\"}"), "s: \"a\\u001fb\"");
        assert_eq!(enc("{\"s\":\"a\\u0000b\"}"), "s: \"a\\u0000b\"");
        assert_eq!(enc("{\"s\":\"\\u0004\"}"), "s: \"\\u0004\"");
    }

    #[test]
    fn test_escape_controls_keeps_named_escapes() {
        assert_eq!(enc(r#"{"s":"a\nb"}"#), "s: \"a\\nb\"");
        assert_eq!(enc(r#"{"s":"a\tb"}"#), "s: \"a\\tb\"");
        assert_eq!(enc(r#"{"s":"a\rb"}"#), "s: \"a\\rb\"");
    }

    #[test]
    fn test_escape_controls_off_passes_raw_byte() {
        let cfg = Config {
            escape_controls: false,
            ..Config::default()
        };
        assert_eq!(enc_with("{\"s\":\"a\\u001fb\"}", &cfg), "s: a\u{1f}b");
    }

    // ── Key folding at depth (spec §13.4) ──

    #[test]
    fn test_fold_keys_root_chain() {
        let cfg = Config {
            key_folding: true,
            ..Config::default()
        };
        assert_eq!(enc_with(r#"{"a":{"b":{"c":1}}}"#, &cfg), "a.b.c: 1");
    }

    #[test]
    fn test_fold_keys_restarts_in_multikey_object_body() {
        // The single-key chain nested→b→c sits inside multi-key object `a`, so
        // folding restarts there and produces `nested.b.c`.
        let cfg = Config {
            key_folding: true,
            ..Config::default()
        };
        assert_eq!(
            enc_with(r#"{"a":{"x":1,"nested":{"b":{"c":2}}}}"#, &cfg),
            "a:\n  x: 1\n  nested.b.c: 2"
        );
    }

    #[test]
    fn test_fold_keys_does_not_refold_past_flatten_depth() {
        let cfg = Config {
            key_folding: true,
            flatten_depth: Some(2),
            ..Config::default()
        };
        assert_eq!(
            enc_with(r#"{"a":{"b":{"c":{"d":1}}}}"#, &cfg),
            "a.b:\n  c:\n    d: 1"
        );
    }

    #[test]
    fn test_fold_keys_skips_sibling_collision_at_any_depth() {
        // A top-level literal `data.meta.items` blocks folding the whole chain.
        let cfg = Config {
            key_folding: true,
            ..Config::default()
        };
        assert_eq!(
            enc_with(
                r#"{"data":{"meta":{"items":[1,2]}},"data.meta.items":"literal"}"#,
                &cfg
            ),
            "data:\n  meta:\n    items[2]: 1,2\ndata.meta.items: literal"
        );
    }
}
