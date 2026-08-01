//! TOON encoder core (sonic-rs backend).
//!
//! Input: JSON bytes (from orjson.dumps on Python side).
//! Output: TOON string, matching TOON spec v4.1.
//!
//! Delimiter is monomorphized via const generics (`DELIM: u8`) so the
//! byte-match inner loops fold away when emitting default-comma output.

use sonic_rs::{Array, JsonContainerTrait, JsonType, JsonValueTrait, Object, Value};
use std::fmt::Write as _;

/// Encoder configuration. The spec's only encoder options are `delimiter` and
/// `indentSize` (§13); the rest are etoon extensions or resource guards.
#[derive(Clone, Copy)]
pub struct Config {
    /// Delimiter between array/tabular values. Must be `,`, `\t`, or `|`.
    pub delimiter: u8,
    /// If true, fold single-key object chains into dot-notation keys (safe
    /// mode). An etoon extension: the spec removed key folding in v4.0, so
    /// nothing re-nests the output.
    pub key_folding: bool,
    /// Max fold depth (segments). None = unlimited. 0 disables folding.
    pub flatten_depth: Option<usize>,
    /// If true, emit empty arrays as canonical `[]` / `key: []` instead of the
    /// legacy `[0]:` / `key[0]:` length-marker form. False emits output the
    /// spec has forbidden since v3.1.
    pub empty_array_bare: bool,
    /// If true, escape control chars U+0000–U+001F (except the named `\n` `\r`
    /// `\t`) as `\uXXXX` with lowercase hex. False emits output the spec has
    /// forbidden since v3.1.
    pub escape_controls: bool,
    /// Max JSON nesting depth. Input deeper than this is rejected before
    /// parsing, so neither the sonic-rs DOM parser nor the recursive emitter
    /// can overflow the stack (both crash the host process near depth ~50k).
    /// 0 disables the check — use only when the input's depth is already
    /// bounded by the producer (e.g. orjson output, capped by CPython's
    /// recursion limit), since the pre-scan is then redundant.
    pub max_depth: usize,
    /// Max input size in bytes. 0 disables the check (default). A caller that
    /// encodes untrusted input can set this to bound peak memory.
    pub max_input_bytes: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            delimiter: b',',
            key_folding: false,
            flatten_depth: None,
            empty_array_bare: true,
            escape_controls: true,
            max_depth: 1000,
            max_input_bytes: 0,
        }
    }
}

pub fn encode(json_bytes: &[u8]) -> Result<String, String> {
    encode_with(json_bytes, &Config::default())
}

pub fn encode_with(json_bytes: &[u8], cfg: &Config) -> Result<String, String> {
    if cfg.max_input_bytes != 0 && json_bytes.len() > cfg.max_input_bytes {
        return Err(format!(
            "input exceeds max_input_bytes ({} > {})",
            json_bytes.len(),
            cfg.max_input_bytes
        ));
    }
    // Reject over-deep input up front: the sonic-rs DOM parser and this
    // emitter both recurse per nesting level and overflow the stack on
    // deeply-nested input. This O(n) pre-scan caps depth before either runs.
    // max_depth == 0 skips it (caller guarantees depth is already bounded).
    if cfg.max_depth != 0 {
        if let Some(depth) = scan_exceeds_depth(json_bytes, cfg.max_depth) {
            return Err(format!(
                "input exceeds max_depth ({} > {})",
                depth, cfg.max_depth
            ));
        }
    }
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
            if let Some(fields) = keyed_fields(m) {
                // Root keyed tabular header is keyless: `[N:]{fields}:` (§9.5).
                write_keyed_table::<DELIM>(m, &fields, 0, cfg, out);
            } else if !m.is_empty() {
                // Folding is attempted at the top-level object; nested object
                // bodies re-apply it via write_value_after_key (spec §13.4).
                write_object_body::<DELIM>(m, 0, cfg, cfg.key_folding, out);
            }
        }
        JsonType::Array => {
            let arr = v.as_array().unwrap();
            // Root empty array: canonical bare `[]` (no leading colon).
            if arr.is_empty() && cfg.empty_array_bare {
                out.push_str("[]");
            } else {
                write_array_suffix::<DELIM>(arr, 0, cfg, true, out);
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
            } else if let Some(fields) = keyed_fields(child) {
                // Keyed tabular form replaces the nested object body; the
                // header attaches directly to the key just written (§9.5).
                write_keyed_table::<DELIM>(child, &fields, key_indent, cfg, out);
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
            // Object value: canonical `key: []`; legacy `key[0]:` otherwise.
            if arr.is_empty() && cfg.empty_array_bare {
                out.push_str(": []");
            } else {
                write_array_suffix::<DELIM>(arr, key_indent, cfg, true, out);
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

/// Emit a field list `{f1<delim>f2{sub}<delim>…}` for a tabular or keyed header,
/// recursing into nested field groups (§9.3).
fn write_field_list<const DELIM: u8>(fields: &[Field], cfg: &Config, out: &mut String) {
    out.push('{');
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(DELIM as char);
        }
        match f {
            Field::Leaf(k) => write_key(k, cfg, out),
            Field::Group(k, sub) => {
                write_key(k, cfg, out);
                write_field_list::<DELIM>(sub, cfg, out);
            }
        }
    }
    out.push('}');
}

/// Emit one row's cells in depth-first pre-order of the field list, so the cell
/// count equals the header's leaf-field count (§9.3).
fn write_row_cells<const DELIM: u8>(
    m: &Object,
    fields: &[Field],
    cfg: &Config,
    first: &mut bool,
    out: &mut String,
) {
    for (idx, f) in fields.iter().enumerate() {
        match f {
            Field::Leaf(k) => {
                if !*first {
                    out.push(DELIM as char);
                }
                *first = false;
                write_scalar::<DELIM>(column_value(m, idx, k).unwrap(), cfg, out);
            }
            Field::Group(k, sub) => {
                let child = column_value(m, idx, k).unwrap().as_object().unwrap();
                write_row_cells::<DELIM>(child, sub, cfg, first, out);
            }
        }
    }
}

/// Emit the keyed tabular body `[N:<delim?>]{fields}:` plus one entry row per
/// entry (§9.5). The caller has already written the key, if any — at the root
/// the header is keyless.
fn write_keyed_table<const DELIM: u8>(
    m: &Object,
    fields: &[Field],
    indent: usize,
    cfg: &Config,
    out: &mut String,
) {
    out.push('[');
    let mut len_buf = itoa::Buffer::new();
    out.push_str(len_buf.format(m.len()));
    out.push(':');
    if DELIM != b',' {
        out.push(DELIM as char);
    }
    out.push(']');
    write_field_list::<DELIM>(fields, cfg, out);
    out.push(':');

    for (k, v) in m.iter() {
        out.push('\n');
        write_indent(indent + 1, out);
        write_key(k, cfg, out);
        out.push_str(": ");
        let mut first = true;
        write_row_cells::<DELIM>(v.as_object().unwrap(), fields, cfg, &mut first, out);
    }
}

fn write_array_suffix<const DELIM: u8>(
    arr: &Array,
    indent: usize,
    cfg: &Config,
    allow_tabular: bool,
    out: &mut String,
) {
    if arr.is_empty() {
        write_empty_array_legacy::<DELIM>(out);
        return;
    }

    out.push('[');
    let mut len_buf = itoa::Buffer::new();
    out.push_str(len_buf.format(arr.len()));
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

    // A keyless fields-bearing header is valid only at the document root (§6),
    // so an array sitting in list-item position takes list form even when its
    // elements would otherwise be tabular-eligible (§9.4).
    let shape = if allow_tabular {
        table_shape(arr)
    } else {
        None
    };

    if let Some(Table::Nested(fields)) = &shape {
        write_field_list::<DELIM>(fields, cfg, out);
        out.push(':');
        for item in arr.iter() {
            out.push('\n');
            write_indent(indent + 1, out);
            let mut first = true;
            write_row_cells::<DELIM>(item.as_object().unwrap(), fields, cfg, &mut first, out);
        }
        return;
    }

    if let Some(Table::Flat(keys, uniform_order)) = shape {
        // Writes the field list inline rather than through write_field_list:
        // flat tables are the hot path, and routing them through `Field` would
        // allocate a tree for a list of names that are all leaves.
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
            // List-item position: no keyless tabular header here (§9.4).
            write_array_suffix::<DELIM>(v.as_array().unwrap(), l, cfg, false, out);
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

// ==================== Depth guard ====================

/// Per-byte structural class for the depth scanner. Most bytes are `Other`
/// (digits, whitespace, separators, string content) and cost a single table
/// lookup + skip, so the scan stays close to memory bandwidth.
const OPEN: u8 = 1;
const CLOSE: u8 = 2;
const QUOTE: u8 = 3;

const CLASS: [u8; 256] = {
    let mut t = [0u8; 256];
    t[b'{' as usize] = OPEN;
    t[b'[' as usize] = OPEN;
    t[b'}' as usize] = CLOSE;
    t[b']' as usize] = CLOSE;
    t[b'"' as usize] = QUOTE;
    t
};

/// Single linear pass over the raw JSON bytes tracking `{`/`[` nesting depth,
/// skipping brackets inside string literals. Returns `Some(depth)` with the
/// first depth that exceeds `max_depth`, or `None` if the input stays within
/// bounds. No allocation; bails out as soon as the limit is crossed.
///
/// String interiors are skipped with `memchr` (SIMD), so quoted content — the
/// bulk of typical payloads — costs near-zero, and the scalar loop only sees
/// structural bytes.
fn scan_exceeds_depth(bytes: &[u8], max_depth: usize) -> Option<usize> {
    let mut depth: usize = 0;
    let mut i = 0;
    let n = bytes.len();
    while i < n {
        match CLASS[bytes[i] as usize] {
            OPEN => {
                depth += 1;
                if depth > max_depth {
                    return Some(depth);
                }
                i += 1;
            }
            CLOSE => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            QUOTE => {
                // Skip to the closing quote, honoring backslash escapes. Each
                // memchr2 jumps straight to the next `"` or `\`.
                i += 1;
                loop {
                    // No `"` or `\` left: the string is unterminated, so there
                    // is no further nesting to find.
                    let p = memchr::memchr2(b'"', b'\\', &bytes[i..])?;
                    if bytes[i + p] == b'"' {
                        i += p + 1;
                        break;
                    }
                    // backslash: skip the escaped byte
                    i += p + 2;
                    if i >= n {
                        return None;
                    }
                }
            }
            _ => i += 1,
        }
    }
    None
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

/// One column of a tabular header (spec §9.3): a bare leaf field, or a nested
/// field group whose sub-columns are themselves leaves or groups. Nesting depth
/// is unbounded.
enum Field<'a> {
    Leaf(&'a str),
    Group(&'a str, Vec<Field<'a>>),
}

/// Tabular shape of an array of objects (§9.3).
enum Table<'a> {
    /// Every column is uniform-primitive. The flag records whether all rows
    /// share the first row's key order, letting cells be emitted by iterating
    /// values in place instead of looking each key up.
    Flat(Vec<&'a str>, bool),
    /// At least one nested-uniform column, emitted as a nested field group.
    Nested(Vec<Field<'a>>),
}

/// Value of column `k` in `m`. Rows normally share the header's key order, so
/// try position `idx` first and fall back to a lookup only when it differs.
#[inline]
fn column_value<'a>(m: &'a Object, idx: usize, k: &str) -> Option<&'a Value> {
    match m.iter().nth(idx) {
        Some((ik, iv)) if ik == k => Some(iv),
        _ => m.get(&k),
    }
}

/// First-row probe for the §9.3 column rules: an array value or an empty object
/// disqualifies its column outright, so a mismatch is visible from one object
/// alone. Callers use it to bail in O(columns) before collecting every row;
/// `build_fields` re-checks each column itself.
#[inline]
fn columns_could_be_uniform(first: &Object) -> bool {
    !first.is_empty()
        && first.iter().all(|(_, v)| match v.get_type() {
            JsonType::Array => false,
            JsonType::Object => !v.as_object().unwrap().is_empty(),
            _ => true,
        })
}

/// Field tree shared by `objs` (§9.3 column classification), or None when any
/// column is neither uniform-primitive nor nested-uniform. Also used for the
/// entry values of a keyed tabular object (§9.5).
fn build_fields<'a>(objs: &[&'a Object]) -> Option<Vec<Field<'a>>> {
    let first = *objs.first()?;
    if first.is_empty() {
        return None;
    }
    for m in &objs[1..] {
        if m.len() != first.len() {
            return None;
        }
    }

    let mut fields = Vec::with_capacity(first.len());
    for (idx, (k, v0)) in first.iter().enumerate() {
        match v0.get_type() {
            JsonType::Object => {
                let sub0 = v0.as_object().unwrap();
                if sub0.is_empty() {
                    return None;
                }
                let mut subs = Vec::with_capacity(objs.len());
                subs.push(sub0);
                for m in &objs[1..] {
                    let sub = column_value(m, idx, k)?.as_object()?;
                    if sub.is_empty() {
                        return None;
                    }
                    subs.push(sub);
                }
                fields.push(Field::Group(k, build_fields(&subs)?));
            }
            // Arrays disqualify the column outright; so does any row whose
            // value at this key is not a primitive.
            JsonType::Array => return None,
            _ => {
                for m in &objs[1..] {
                    if !is_scalar(column_value(m, idx, k)?) {
                        return None;
                    }
                }
                fields.push(Field::Leaf(k));
            }
        }
    }
    Some(fields)
}

fn table_shape<'a>(arr: &'a Array) -> Option<Table<'a>> {
    if let Some((keys, uniform_order)) = table_keys(arr) {
        return Some(Table::Flat(keys, uniform_order));
    }
    // Flat detection bails at the first non-primitive value, but a column of
    // uniform objects still qualifies as a nested field group (§9.3), so retry
    // with the recursive walk. Probe the first element before walking all of
    // them: with no object column there is nothing the flat pass missed, and a
    // disqualifying value is usually already visible here — that keeps the
    // common mixed-array case (a tabular-looking array with one list column)
    // from paying for a full scan on its way to list form.
    let probe = arr.iter().next()?.as_object()?;
    if !columns_could_be_uniform(probe)
        || !probe
            .iter()
            .any(|(_, v)| matches!(v.get_type(), JsonType::Object))
    {
        return None;
    }

    let mut objs = Vec::with_capacity(arr.len());
    for v in arr.iter() {
        objs.push(v.as_object()?);
    }
    Some(Table::Nested(build_fields(&objs)?))
}

/// Field tree when `m` qualifies for keyed tabular form (§9.5): at least two
/// entries, every entry value a non-empty object, one shared key set, and every
/// column uniform-primitive or nested-uniform.
fn keyed_fields<'a>(m: &'a Object) -> Option<Vec<Field<'a>>> {
    if m.len() < 2 {
        return None;
    }
    // Cheap reject before allocating: most objects fail on their first entry.
    let probe = m.iter().next()?.1.as_object()?;
    if !columns_could_be_uniform(probe) {
        return None;
    }
    let mut objs = Vec::with_capacity(m.len());
    for (_, v) in m.iter() {
        objs.push(v.as_object()?);
    }
    build_fields(&objs)
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
    // ryu is ~2.4x faster than std Display for non-integer floats, but emits
    // scientific notation for very small/large magnitudes (1e-6, 1e21) which
    // violates TOON's expanded-decimal form. Use ryu when its output has no
    // exponent (the common LLM-payload case); otherwise fall back to std
    // Display, which always expands.
    let mut buf = ryu::Buffer::new();
    let s = buf.format_finite(f);
    if s.as_bytes().contains(&b'e') {
        // std Display gives spec-canonical decimals (expanded, no trailing zeros).
        write!(out, "{}", f).unwrap();
    } else {
        out.push_str(s);
    }
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
        b'-' | b'#' | b' ' | b'\t' => return true,
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
            // Brackets and braces anywhere in the value, not just at position 0
            // (spec §7.2) — an unquoted `]` would otherwise close a header the
            // decoder is scanning.
            b':' | b'\n' | b'\r' | b'\t' | b'"' | b'\\' | b'[' | b']' | b'{' | b'}' => return true,
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

/// Numeric-like per spec §7.2: `^[+-]?[0-9]+(?:\.[0-9]+)?(?:e[+-]?[0-9]+)?$`.
/// The leading sign includes `+`, so `"+1"` is quoted and survives round-trip.
fn looks_like_number(bytes: &[u8]) -> bool {
    let mut i = 0;
    if matches!(bytes[0], b'-' | b'+') {
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

    // ── Keyed tabular form (spec §9.5) ──
    // The happy paths live in tests/fixtures/encode/objects-keyed.json; these
    // pin the detection boundaries, where the object must stay nested.

    #[test]
    fn test_keyed_table_needs_two_entries() {
        // A single entry stays nested — the header would cost more than it saves.
        assert_eq!(enc(r#"{"m":{"a":{"x":1}}}"#), "m:\n  a:\n    x: 1");
        assert_eq!(
            enc(r#"{"m":{"a":{"x":1},"b":{"x":2}}}"#),
            "m[2:]{x}:\n  a: 1\n  b: 2"
        );
    }

    #[test]
    fn test_keyed_table_rejects_non_uniform_columns() {
        // Mismatched key sets, a non-object entry, and an array column each
        // disqualify the whole object (§9.5 detection).
        assert_eq!(
            enc(r#"{"m":{"a":{"x":1},"b":{"y":2}}}"#),
            "m:\n  a:\n    x: 1\n  b:\n    y: 2"
        );
        assert_eq!(
            enc(r#"{"m":{"a":{"x":1},"b":7}}"#),
            "m:\n  a:\n    x: 1\n  b: 7"
        );
        assert_eq!(
            enc(r#"{"m":{"a":{"x":[1]},"b":{"x":[2]}}}"#),
            "m:\n  a:\n    x[1]: 1\n  b:\n    x[1]: 2"
        );
    }

    #[test]
    fn test_keyed_table_not_used_for_array_elements() {
        // The `q` column mixes an object with an array, so the array takes list
        // form. Its first element is keyed-eligible on its own (two entries,
        // one shared key set) but stays nested: array elements are anonymous
        // and there is no `- [N:]{…}:` list item (§9.5, §10).
        assert_eq!(
            enc(r#"{"a":[{"p":{"x":1},"q":{"x":2}},{"p":{"x":3},"q":[9]}]}"#),
            "a[2]:\n  - p:\n      x: 1\n    q:\n      x: 2\n  - p:\n      x: 3\n    q[1]: 9"
        );
    }

    #[test]
    fn test_keyed_eligible_column_becomes_nested_field_group() {
        // In a tabular column, a keyed-eligible object encodes as a nested
        // field group rather than a keyed table (§9.5).
        assert_eq!(
            enc(r#"{"a":[{"p":{"x":1},"q":{"x":2}}]}"#),
            "a[1]{p{x},q{x}}:\n  1,2"
        );
    }

    // ── Nested field groups (spec §9.3) ──

    #[test]
    fn test_nested_field_group_rejects_empty_object_column() {
        // A column of empty objects has no subfields to declare, so the array
        // falls back to list form.
        assert_eq!(enc(r#"{"a":[{"n":{}},{"n":{}}]}"#), "a[2]:\n  - n:\n  - n:");
    }

    #[test]
    fn test_nested_field_group_rejects_mixed_null_and_object_column() {
        // `null` is a primitive, so the column is neither uniform-primitive nor
        // nested-uniform (§9.3) and the array takes list form.
        assert_eq!(
            enc(r#"{"a":[{"n":{"x":1}},{"n":null}]}"#),
            "a[2]:\n  - n:\n      x: 1\n  - n: null"
        );
    }

    #[test]
    fn test_nested_field_group_tolerates_row_key_reordering() {
        // Key order may vary per element; cells still follow the header order.
        assert_eq!(
            enc(r#"{"a":[{"id":1,"g":{"x":1,"y":2}},{"g":{"y":4,"x":3},"id":2}]}"#),
            "a[2]{id,g{x,y}}:\n  1,1,2\n  2,3,4"
        );
    }

    // ── String quoting (spec §7.2) ──

    #[test]
    fn test_quotes_leading_plus_numeric_like_string() {
        assert_eq!(enc(r#"{"a":"+1"}"#), r#"a: "+1""#);
        assert_eq!(enc(r#"{"a":"+1.5e-3"}"#), r#"a: "+1.5e-3""#);
        // A plus that does not form a number stays unquoted.
        assert_eq!(enc(r#"{"a":"+x"}"#), "a: +x");
    }

    #[test]
    fn test_quotes_brackets_and_braces_anywhere_in_value() {
        assert_eq!(enc(r#"{"a":"x[1]"}"#), r#"a: "x[1]""#);
        assert_eq!(enc(r#"{"a":"a}b"}"#), r#"a: "a}b""#);
    }

    // ── Depth guard (P0: prevents sonic-rs/emitter stack overflow) ──

    #[test]
    fn test_max_depth_rejects_overdeep_input_before_parse() {
        // Depth far below the ~50k crash threshold, but past a small limit:
        // must return Err, never overflow the stack.
        let deep: Vec<u8> = b"["
            .iter()
            .cycle()
            .take(100)
            .chain(b"1".iter())
            .chain(b"]".iter().cycle().take(100))
            .copied()
            .collect();
        let cfg = Config {
            max_depth: 10,
            ..Config::default()
        };
        let err = encode_with(&deep, &cfg).unwrap_err();
        assert!(err.contains("max_depth"), "got: {err}");
    }

    #[test]
    fn test_max_depth_default_allows_normal_nesting() {
        // Ordinary nesting (well under default 1000) encodes fine.
        assert_eq!(enc(r#"{"a":{"b":{"c":1}}}"#), "a:\n  b:\n    c: 1");
    }

    #[test]
    fn test_max_depth_ignores_brackets_inside_strings() {
        // Brackets in string literals must not count toward depth.
        let cfg = Config {
            max_depth: 2,
            ..Config::default()
        };
        assert_eq!(
            enc_with(r#"{"s":"[[[[[deep]]]]]"}"#, &cfg),
            r#"s: "[[[[[deep]]]]]""#
        );
    }

    // ── Input-size guard (P1: OOM protection, off by default) ──

    #[test]
    fn test_max_input_bytes_rejects_oversize_input() {
        let cfg = Config {
            max_input_bytes: 4,
            ..Config::default()
        };
        let err = encode_with(br#"{"a":1}"#, &cfg).unwrap_err();
        assert!(err.contains("max_input_bytes"), "got: {err}");
    }

    #[test]
    fn test_max_input_bytes_zero_disables_check() {
        // Default (0) imposes no limit.
        assert_eq!(enc(r#"{"a":1}"#), "a: 1");
    }

    // ── ryu float fast path keeps spec-canonical output ──

    #[test]
    fn test_ryu_regular_floats_match_spec_form() {
        // Common-range floats go through ryu (no exponent) and stay expanded.
        assert_eq!(enc(r#"{"n":2.5}"#), "n: 2.5");
        assert_eq!(enc(r#"{"n":99.99}"#), "n: 99.99");
        assert_eq!(enc(r#"{"n":0.1}"#), "n: 0.1");
        assert_eq!(enc(r#"{"n":-0.0625}"#), "n: -0.0625");
    }
}
