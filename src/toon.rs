//! TOON encoder core (sonic-rs backend).
//!
//! Input: JSON bytes (from orjson.dumps on Python side).
//! Output: TOON string, byte-identical to `toons.dumps()` for standard JSON payloads.

use sonic_rs::{Array, JsonContainerTrait, JsonType, JsonValueTrait, Object, Value};
use std::fmt::Write as _;

pub fn encode(json_bytes: &[u8]) -> Result<String, String> {
    let value: Value =
        sonic_rs::from_slice(json_bytes).map_err(|e| format!("JSON parse error: {}", e))?;
    // TOON output is always ≤ input JSON size; use input.len() as a safe upper bound.
    let mut out = String::with_capacity(json_bytes.len());
    write_root(&value, &mut out);
    Ok(out)
}

fn write_root(v: &Value, out: &mut String) {
    match v.get_type() {
        JsonType::Object => {
            let m = v.as_object().unwrap();
            if !m.is_empty() {
                write_object_body(m, 0, out);
            }
        }
        JsonType::Array => write_array_suffix(v.as_array().unwrap(), 0, out),
        _ => write_scalar(v, out),
    }
}

fn write_object_body(m: &Object, indent: usize, out: &mut String) {
    let mut first = true;
    for (k, v) in m.iter() {
        if !first {
            out.push('\n');
        }
        first = false;
        write_indent(indent, out);
        write_key_value(k, v, indent, out);
    }
}

fn write_key_value(k: &str, v: &Value, indent: usize, out: &mut String) {
    write_key(k, out);
    write_value_after_key(v, indent, out);
}

/// Write the ": value" or ":\n<body>" tail after a key at `key_indent`.
/// Child object bodies go at `key_indent + 1`; array rows go at `key_indent + 1`
/// (via write_array_suffix's internal `+ 1`).
fn write_value_after_key(v: &Value, key_indent: usize, out: &mut String) {
    match v.get_type() {
        JsonType::Object => {
            let child = v.as_object().unwrap();
            if child.is_empty() {
                out.push(':');
            } else {
                out.push_str(":\n");
                write_object_body(child, key_indent + 1, out);
            }
        }
        JsonType::Array => write_array_suffix(v.as_array().unwrap(), key_indent, out),
        _ => {
            out.push_str(": ");
            write_scalar(v, out);
        }
    }
}

fn write_array_suffix(arr: &Array, indent: usize, out: &mut String) {
    write!(out, "[{}]", arr.len()).unwrap();

    if arr.is_empty() {
        out.push(':');
        return;
    }

    if arr.iter().all(is_scalar) {
        out.push_str(": ");
        let mut first = true;
        for v in arr.iter() {
            if !first {
                out.push(',');
            }
            first = false;
            write_scalar(v, out);
        }
        return;
    }

    if let Some((keys, uniform_order)) = table_keys(arr) {
        out.push('{');
        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            write_key(k, out);
        }
        out.push_str("}:");
        if uniform_order {
            // Fast path: all rows have keys in the same order as header.
            // Iterate sequentially, no key lookups.
            for item in arr.iter() {
                let m = item.as_object().unwrap();
                out.push('\n');
                write_indent(indent + 1, out);
                let mut first = true;
                for (_, v) in m.iter() {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    write_scalar(v, out);
                }
            }
        } else {
            // Slow path: row orders differ, lookup per key.
            for item in arr.iter() {
                let m = item.as_object().unwrap();
                out.push('\n');
                write_indent(indent + 1, out);
                let mut first = true;
                for k in &keys {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    write_scalar(m.get(k).unwrap(), out);
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
        write_list_item(item, indent + 1, out);
    }
}

fn write_list_item(v: &Value, l: usize, out: &mut String) {
    match v.get_type() {
        JsonType::Object => {
            let m = v.as_object().unwrap();
            if !m.is_empty() {
                out.push(' ');
                write_list_item_object(m, l, out);
            }
        }
        JsonType::Array => {
            out.push(' ');
            write_array_suffix(v.as_array().unwrap(), l, out);
        }
        _ => {
            out.push(' ');
            write_scalar(v, out);
        }
    }
}

fn write_list_item_object(m: &Object, l: usize, out: &mut String) {
    let mut first = true;
    for (k, v) in m.iter() {
        if !first {
            out.push('\n');
            write_indent(l + 1, out);
        }
        first = false;
        write_key(k, out);
        // List-item's first key sits at virtual indent l+1, so pass l+1 as key_indent.
        write_value_after_key(v, l + 1, out);
    }
}

// ==================== Helpers ====================

// Pre-computed indent strings for common depths (0-8 levels).
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

/// Return ordered keys + order-uniformity flag if array is tabular-eligible.
/// `uniform_order = true` means every row has keys in the exact same order as the header,
/// allowing sequential iteration without key lookups.
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
        // Order mismatch: re-verify via lookup that every header key exists in this row.
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

fn write_scalar(v: &Value, out: &mut String) {
    match v.get_type() {
        JsonType::Null => out.push_str("null"),
        JsonType::Boolean => out.push_str(if v.as_bool().unwrap() {
            "true"
        } else {
            "false"
        }),
        JsonType::Number => write_number(v, out),
        JsonType::String => write_string_value(v.as_str().unwrap(), out),
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

fn write_string_value(s: &str, out: &mut String) {
    if needs_quoting(s, false) {
        write_quoted(s, out);
    } else {
        out.push_str(s);
    }
}

fn write_key(k: &str, out: &mut String) {
    if needs_quoting(k, true) {
        write_quoted(k, out);
    } else {
        out.push_str(k);
    }
}

fn needs_quoting(s: &str, is_key: bool) -> bool {
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
    for &b in bytes {
        match b {
            b',' | b':' | b'\n' | b'\r' | b'\t' | b'"' | b'\\' => return true,
            b' ' if is_key => return true,
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
    // Fast path: bulk-copy spans between escape chars using memchr-like scan.
    // Escape bytes: '\\' 0x5c, '"' 0x22, '\n' 0x0a, '\r' 0x0d, '\t' 0x09
    out.push('"');
    let bytes = s.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if matches!(b, b'\\' | b'"' | b'\n' | b'\r' | b'\t') {
            if start < i {
                // SAFETY: start..i is bounded by ASCII escape char positions;
                // UTF-8 boundaries are preserved since escape chars are single-byte ASCII.
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
