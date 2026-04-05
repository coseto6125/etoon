//! etoon CLI: read JSON from stdin (or file), write TOON to stdout.
//!
//! Usage:
//!   command | etoon                  # stdin → stdout (auto JSON/log)
//!   etoon input.json                 # file → stdout
//!   etoon input.json -o output.toon  # file → file
//!   etoon --strict                   # error on non-JSON (old behavior)

use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;
    let mut strict = false;

    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--output" => {
                output_path = args.next();
                if output_path.is_none() {
                    eprintln!("etoon: -o requires an argument");
                    return ExitCode::FAILURE;
                }
            }
            "--strict" => {
                strict = true;
            }
            "-h" | "--help" => {
                println!("etoon: TOON encoder");
                println!("usage: etoon [INPUT] [-o OUTPUT] [--strict]");
                println!("       command | etoon           # auto JSON/log");
                println!("       etoon --strict            # error on non-JSON");
                return ExitCode::SUCCESS;
            }
            _ if !a.starts_with('-') && input_path.is_none() => {
                input_path = Some(a);
            }
            _ => {
                eprintln!("etoon: unknown argument: {}", a);
                return ExitCode::FAILURE;
            }
        }
    }

    match input_path {
        Some(path) => run_file_mode(&path, output_path),
        None if strict => run_bulk_stdin(output_path, true),
        None => run_auto_stdin(output_path),
    }
}

/// File mode: read entire file as JSON, encode to TOON.
fn run_file_mode(path: &str, output_path: Option<String>) -> ExitCode {
    let json_bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("etoon: cannot read {}: {}", path, e);
            return ExitCode::FAILURE;
        }
    };
    let toon = match _etoon::toon::encode(&json_bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("etoon: encode error: {}", e);
            return ExitCode::FAILURE;
        }
    };
    write_output(&toon, output_path)
}

/// Auto stdin: peek first non-whitespace byte to decide path.
/// - Starts with `{` or `[` → likely pure JSON → bulk encode (fallback to log on error)
/// - Otherwise → definitely mixed log → line-by-line directly
fn run_auto_stdin(output_path: Option<String>) -> ExitCode {
    let mut buf = Vec::with_capacity(8192);
    if let Err(e) = io::stdin().lock().read_to_end(&mut buf) {
        eprintln!("etoon: stdin read error: {}", e);
        return ExitCode::FAILURE;
    }

    let first = buf.iter().find(|b| !b.is_ascii_whitespace());
    match first {
        Some(b'{') | Some(b'[') => {
            // Likely pure JSON — try bulk first
            match _etoon::toon::encode(&buf) {
                Ok(toon) => write_output(&toon, output_path),
                Err(_) => run_log_from_bytes(&buf, output_path),
            }
        }
        _ => {
            // Starts with non-JSON char — go straight to log mode
            run_log_from_bytes(&buf, output_path)
        }
    }
}

/// Strict stdin mode: read all stdin as one JSON blob, error on non-JSON.
fn run_bulk_stdin(output_path: Option<String>, strict: bool) -> ExitCode {
    let mut buf = Vec::with_capacity(8192);
    if let Err(e) = io::stdin().lock().read_to_end(&mut buf) {
        eprintln!("etoon: stdin read error: {}", e);
        return ExitCode::FAILURE;
    }
    match _etoon::toon::encode(&buf) {
        Ok(toon) => write_output(&toon, output_path),
        Err(e) if strict => {
            eprintln!("etoon: encode error: {}", e);
            ExitCode::FAILURE
        }
        Err(_) => run_log_from_bytes(&buf, output_path),
    }
}

fn write_output(toon: &str, output_path: Option<String>) -> ExitCode {
    let mut out: Box<dyn Write> = match output_path {
        Some(path) => match fs::File::create(&path) {
            Ok(f) => Box::new(io::BufWriter::new(f)),
            Err(e) => {
                eprintln!("etoon: cannot create {}: {}", path, e);
                return ExitCode::FAILURE;
            }
        },
        None => Box::new(io::BufWriter::new(io::stdout().lock())),
    };
    if let Err(e) = out
        .write_all(toon.as_bytes())
        .and_then(|_| out.write_all(b"\n"))
    {
        eprintln!("etoon: write error: {}", e);
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Log mode from pre-read buffer (fallback when bulk parse fails).
fn run_log_from_bytes(buf: &[u8], output_path: Option<String>) -> ExitCode {
    let mut out: Box<dyn Write> = match output_path {
        Some(path) => match fs::File::create(&path) {
            Ok(f) => Box::new(io::BufWriter::new(f)),
            Err(e) => {
                eprintln!("etoon: cannot create {}: {}", path, e);
                return ExitCode::FAILURE;
            }
        },
        None => Box::new(io::BufWriter::new(io::stdout().lock())),
    };

    let mut json_buf = String::new();
    let mut brace_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut in_json_block = false;

    for line in buf.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("etoon: read error: {}", e);
                return ExitCode::FAILURE;
            }
        };

        if in_json_block {
            update_depths(&line, &mut brace_depth, &mut bracket_depth);
            json_buf.push('\n');
            json_buf.push_str(&line);

            if brace_depth <= 0 && bracket_depth <= 0 {
                in_json_block = false;
                let encoded = try_encode_json(json_buf.trim().as_bytes());
                let text = encoded.as_deref().unwrap_or(&json_buf);
                if let Err(e) = writeln!(out, "{}", text) {
                    eprintln!("etoon: write error: {}", e);
                    return ExitCode::FAILURE;
                }
                json_buf.clear();
                brace_depth = 0;
                bracket_depth = 0;
            }
            continue;
        }

        let trimmed = line.trim_start();

        if trimmed.starts_with('{') || looks_like_json_array(trimmed) {
            if let Some(encoded) = try_encode_json(trimmed.as_bytes()) {
                if let Err(e) = writeln!(out, "{}", encoded) {
                    eprintln!("etoon: write error: {}", e);
                    return ExitCode::FAILURE;
                }
            } else {
                in_json_block = true;
                json_buf.clear();
                json_buf.push_str(&line);
                brace_depth = 0;
                bracket_depth = 0;
                update_depths(&line, &mut brace_depth, &mut bracket_depth);
            }
            continue;
        }

        if let Err(e) = writeln!(out, "{}", line) {
            eprintln!("etoon: write error: {}", e);
            return ExitCode::FAILURE;
        }
    }

    if !json_buf.is_empty() {
        if let Err(e) = writeln!(out, "{}", json_buf) {
            eprintln!("etoon: write error: {}", e);
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

fn try_encode_json(bytes: &[u8]) -> Option<String> {
    _etoon::toon::encode(bytes).ok()
}

/// Distinguish JSON array `[{...}]` / `[1,2]` from log prefixes like `[request-id]`.
/// Strategy: if `[...]` closes on the same segment and is followed by non-JSON chars → log prefix.
/// Otherwise check if content after `[` looks like JSON values.
fn looks_like_json_array(trimmed: &str) -> bool {
    if !trimmed.starts_with('[') {
        return false;
    }
    // Find the first `]` — if it closes quickly and has trailing non-JSON text, it's a log prefix
    if let Some(close_pos) = trimmed.find(']') {
        let after_close = trimmed[close_pos + 1..].trim_start();
        // `[something] more text` → log prefix, not JSON
        if !after_close.is_empty() && !after_close.starts_with(',') && !after_close.starts_with(']')
        {
            return false;
        }
    }
    let after_bracket = trimmed[1..].trim_start();
    if after_bracket.is_empty() {
        return true; // `[` alone on a line → likely multi-line array
    }
    let first = after_bracket.as_bytes()[0];
    matches!(
        first,
        b'{' | b'[' | b'"' | b'0'..=b'9' | b'-' | b't' | b'f' | b'n' | b']'
    )
}

fn update_depths(line: &str, brace_depth: &mut i32, bracket_depth: &mut i32) {
    let mut in_string = false;
    let mut prev_backslash = false;
    for ch in line.chars() {
        if in_string {
            if ch == '"' && !prev_backslash {
                in_string = false;
            }
            prev_backslash = ch == '\\' && !prev_backslash;
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => *brace_depth += 1,
            '}' => *brace_depth -= 1,
            '[' => *bracket_depth += 1,
            ']' => *bracket_depth -= 1,
            _ => {}
        }
        prev_backslash = false;
    }
}
