//! TOON encoder core (sonic-rs backend).
//!
//! Input: JSON bytes (from orjson.dumps on Python side).
//! Output: TOON string, matching TOON spec v1.5.
//!
//! Delimiter is monomorphized via const generics (`DELIM: u8`) so the
//! byte-match inner loops fold away when emitting default-comma output.

use sonic_rs::{Array, JsonContainerTrait, JsonType, JsonValueTrait, Object, Value};
use std::collections::HashSet;
use std::fmt::Write as _;

/// Encoder configuration matching TOON spec v1.5 options.
#[derive(Clone, Copy)]
pub struct Config {
    /// Delimiter between array/tabular values. Must be `,`, `\t`, or `|`.
    pub delimiter: u8,
    /// If true, fold single-key object chains into dot-notation keys (safe mode).
    pub key_folding: bool,
    /// Max fold depth (segments). None = unlimited. 0 disables folding.
    pub flatten_depth: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            delimiter: b',',
            key_folding: false,
            flatten_depth: None,
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
                // Key folding applies only at the top-level object (TOON spec v1.5).
                write_object_body::<DELIM>(m, 0, cfg, cfg.key_folding, out);
            }
        }
        JsonType::Array => write_array_suffix::<DELIM>(v.as_array().unwrap(), 0, cfg, out),
        _ => write_scalar::<DELIM>(v, out),
    }
}

fn write_object_body<const DELIM: u8>(
    m: &Object,
    indent: usize,
    cfg: &Config,
    allow_fold: bool,
    out: &mut String,
) {
    let siblings: Option<HashSet<&str>> = if allow_fold {
        Some(m.iter().map(|(k, _)| k).collect())
    } else {
        None
    };

    let mut first = true;
    for (k, v) in m.iter() {
        if !first {
            out.push('\n');
        }
        first = false;
        write_indent(indent, out);

        if let Some(ref sibs) = siblings {
            if let Some((path, final_v)) = try_fold(k, v, cfg, sibs) {
                for (i, seg) in path.iter().enumerate() {
                    if i > 0 {
                        out.push('.');
                    }
                    out.push_str(seg);
                }
                write_value_after_key::<DELIM>(final_v, indent, cfg, out);
                continue;
            }
        }

        write_key(k, out);
        write_value_after_key::<DELIM>(v, indent, cfg, out);
    }
}

fn try_fold<'a>(
    k: &'a str,
    v: &'a Value,
    cfg: &Config,
    siblings: &HashSet<&str>,
) -> Option<(Vec<&'a str>, &'a Value)> {
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
    for &s in siblings {
        if s != k && s == joined.as_str() {
            return None;
        }
    }

    Some((path, cur_v))
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
                // Nested object bodies never re-apply key folding (TOON spec: top-level only).
                write_object_body::<DELIM>(child, key_indent + 1, cfg, false, out);
            }
        }
        JsonType::Array => write_array_suffix::<DELIM>(v.as_array().unwrap(), key_indent, cfg, out),
        _ => {
            out.push_str(": ");
            write_scalar::<DELIM>(v, out);
        }
    }
}

fn write_array_suffix<const DELIM: u8>(arr: &Array, indent: usize, cfg: &Config, out: &mut String) {
    write!(out, "[{}", arr.len()).unwrap();
    if DELIM != b',' {
        out.push(DELIM as char);
    }
    out.push(']');

    if arr.is_empty() {
        out.push(':');
        return;
    }

    if arr.iter().all(is_scalar) {
        out.push_str(": ");
        let mut first = true;
        for v in arr.iter() {
            if !first {
                out.push(DELIM as char);
            }
            first = false;
            write_scalar::<DELIM>(v, out);
        }
        return;
    }

    if let Some((keys, uniform_order)) = table_keys(arr) {
        out.push('{');
        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                out.push(DELIM as char);
            }
            write_key(k, out);
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
                    write_scalar::<DELIM>(v, out);
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
                    write_scalar::<DELIM>(m.get(k).unwrap(), out);
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
            write_scalar::<DELIM>(v, out);
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
        write_key(k, out);
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
fn write_scalar<const DELIM: u8>(v: &Value, out: &mut String) {
    match v.get_type() {
        JsonType::Null => out.push_str("null"),
        JsonType::Boolean => out.push_str(if v.as_bool().unwrap() {
            "true"
        } else {
            "false"
        }),
        JsonType::Number => write_number(v, out),
        JsonType::String => write_string_value::<DELIM>(v.as_str().unwrap(), out),
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
    let raw = v.to_string();
    if !raw.contains('.') && !raw.contains('e') && !raw.contains('E') {
        out.push_str(&raw);
        return;
    }
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
    if f.fract() == 0.0 && f.abs() < 1e16 {
        let mut buf = itoa::Buffer::new();
        out.push_str(buf.format(f as i64));
        return;
    }
    write!(out, "{}", f).unwrap();
}

// ==================== String ====================

#[inline]
fn write_string_value<const DELIM: u8>(s: &str, out: &mut String) {
    if value_needs_quoting::<DELIM>(s) {
        write_quoted(s, out);
    } else {
        out.push_str(s);
    }
}

fn write_key(k: &str, out: &mut String) {
    if key_needs_quoting(k) {
        write_quoted(k, out);
    } else {
        out.push_str(k);
    }
}

/// Keys must match TOON identifier pattern: `[a-zA-Z_][a-zA-Z0-9_.]*`.
#[inline]
fn key_needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return true;
    }
    for &b in &bytes[1..] {
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
            return true;
        }
    }
    false
}

#[inline]
fn value_needs_quoting<const DELIM: u8>(s: &str) -> bool {
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

fn write_quoted(s: &str, out: &mut String) {
    out.push('"');
    let bytes = s.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if matches!(b, b'\\' | b'"' | b'\n' | b'\r' | b'\t') {
            if start < i {
                out.push_str(unsafe { std::str::from_utf8_unchecked(&bytes[start..i]) });
            }
            out.push_str(match b {
                b'\\' => "\\\\",
                b'"' => "\\\"",
                b'\n' => "\\n",
                b'\r' => "\\r",
                b'\t' => "\\t",
                _ => unreachable!(),
            });
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push_str(unsafe { std::str::from_utf8_unchecked(&bytes[start..]) });
    }
    out.push('"');
}
