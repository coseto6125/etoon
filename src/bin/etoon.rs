//! etoon CLI: read JSON from stdin (or file), write TOON to stdout.
//!
//! Usage:
//!   command | etoon                  # stdin → stdout (auto JSON/log)
//!   etoon input.json                 # file → stdout
//!   etoon input.json -o output.toon  # file → file
//!   etoon --strict                   # error on non-JSON (old behavior)

use std::env;
use std::fs;
use std::io::{self, Read, Write};
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
        None if strict => run_strict_stdin(output_path),
        None => run_auto_stdin(output_path),
    }
}

fn read_stdin() -> Result<Vec<u8>, ExitCode> {
    let mut buf = Vec::with_capacity(8192);
    io::stdin().lock().read_to_end(&mut buf).map_err(|e| {
        eprintln!("etoon: stdin read error: {}", e);
        ExitCode::FAILURE
    })?;
    Ok(buf)
}

fn open_output(output_path: Option<String>) -> Result<Box<dyn Write>, ExitCode> {
    match output_path {
        Some(path) => match fs::File::create(&path) {
            Ok(f) => Ok(Box::new(io::BufWriter::with_capacity(65536, f))),
            Err(e) => {
                eprintln!("etoon: cannot create {}: {}", path, e);
                Err(ExitCode::FAILURE)
            }
        },
        None => Ok(Box::new(io::BufWriter::with_capacity(
            65536,
            io::stdout().lock(),
        ))),
    }
}

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

/// Peek first non-whitespace byte to decide path.
fn run_auto_stdin(output_path: Option<String>) -> ExitCode {
    let buf = match read_stdin() {
        Ok(b) => b,
        Err(code) => return code,
    };

    let first = buf.iter().find(|b| !b.is_ascii_whitespace());
    match first {
        Some(b'{') | Some(b'[') => match _etoon::toon::encode(&buf) {
            Ok(toon) => write_output(&toon, output_path),
            Err(_) => run_log_from_bytes(&buf, output_path),
        },
        _ => run_log_from_bytes(&buf, output_path),
    }
}

fn run_strict_stdin(output_path: Option<String>) -> ExitCode {
    let buf = match read_stdin() {
        Ok(b) => b,
        Err(code) => return code,
    };
    match _etoon::toon::encode(&buf) {
        Ok(toon) => write_output(&toon, output_path),
        Err(e) => {
            eprintln!("etoon: encode error: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// Bulk output — two direct write_all calls, no BufWriter needed.
fn write_output(toon: &str, output_path: Option<String>) -> ExitCode {
    let result = match output_path {
        Some(ref path) => {
            let mut f = match fs::File::create(path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("etoon: cannot create {}: {}", path, e);
                    return ExitCode::FAILURE;
                }
            };
            f.write_all(toon.as_bytes())
                .and_then(|_| f.write_all(b"\n"))
        }
        None => {
            let mut out = io::stdout().lock();
            out.write_all(toon.as_bytes())
                .and_then(|_| out.write_all(b"\n"))
        }
    };
    if let Err(e) = result {
        eprintln!("etoon: write error: {}", e);
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Line-by-line log mode from pre-read buffer.
/// JSON blocks → TOON encode, non-JSON → batch write contiguous ranges.
fn run_log_from_bytes(buf: &[u8], output_path: Option<String>) -> ExitCode {
    let mut out = match open_output(output_path) {
        Ok(w) => w,
        Err(code) => return code,
    };

    let buf_base = buf.as_ptr() as usize;
    // Track contiguous plain-text range for batch writes
    let mut plain_start: Option<usize> = None;
    let mut block_start: usize = 0;
    let mut brace_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut in_json_block = false;

    for raw_line in buf.split(|&b| b == b'\n') {
        let line_offset = raw_line.as_ptr() as usize - buf_base;

        if in_json_block {
            let line = match std::str::from_utf8(raw_line) {
                Ok(s) => s.strip_suffix('\r').unwrap_or(s),
                Err(_) => {
                    // Invalid UTF-8 inside JSON block — abandon block, pass-through
                    in_json_block = false;
                    let block_bytes = &buf[block_start..line_offset + raw_line.len()];
                    if let Err(e) = out
                        .write_all(block_bytes)
                        .and_then(|_| out.write_all(b"\n"))
                    {
                        eprintln!("etoon: write error: {}", e);
                        return ExitCode::FAILURE;
                    }
                    continue;
                }
            };

            update_depths(line, &mut brace_depth, &mut bracket_depth);

            if brace_depth <= 0 && bracket_depth <= 0 {
                in_json_block = false;
                let block_end = line_offset + raw_line.len();
                let block_bytes = &buf[block_start..block_end];
                let block_str = std::str::from_utf8(block_bytes).unwrap_or("");

                if let Some(encoded) = try_encode_json(block_str.trim().as_bytes()) {
                    if let Err(e) = out
                        .write_all(encoded.as_bytes())
                        .and_then(|_| out.write_all(b"\n"))
                    {
                        eprintln!("etoon: write error: {}", e);
                        return ExitCode::FAILURE;
                    }
                } else if let Err(e) = out
                    .write_all(block_str.trim().as_bytes())
                    .and_then(|_| out.write_all(b"\n"))
                {
                    eprintln!("etoon: write error: {}", e);
                    return ExitCode::FAILURE;
                }
                brace_depth = 0;
                bracket_depth = 0;
            }
            continue;
        }

        let line = match std::str::from_utf8(raw_line) {
            Ok(s) => s.strip_suffix('\r').unwrap_or(s),
            Err(_) => {
                // Non-UTF8: flush plain run, then pass-through raw bytes
                if let Some(start) = plain_start.take() {
                    if let Err(e) = out.write_all(&buf[start..line_offset]) {
                        eprintln!("etoon: write error: {}", e);
                        return ExitCode::FAILURE;
                    }
                }
                if let Err(e) = out.write_all(raw_line).and_then(|_| out.write_all(b"\n")) {
                    eprintln!("etoon: write error: {}", e);
                    return ExitCode::FAILURE;
                }
                continue;
            }
        };

        let trimmed = line.trim_start();

        if trimmed.starts_with('{') || looks_like_json_array(trimmed) {
            // Flush accumulated plain text
            if let Some(start) = plain_start.take() {
                if let Err(e) = out.write_all(&buf[start..line_offset]) {
                    eprintln!("etoon: write error: {}", e);
                    return ExitCode::FAILURE;
                }
            }

            if let Some(encoded) = try_encode_json(trimmed.as_bytes()) {
                if let Err(e) = out
                    .write_all(encoded.as_bytes())
                    .and_then(|_| out.write_all(b"\n"))
                {
                    eprintln!("etoon: write error: {}", e);
                    return ExitCode::FAILURE;
                }
            } else {
                in_json_block = true;
                block_start = line_offset;
                brace_depth = 0;
                bracket_depth = 0;
                update_depths(line, &mut brace_depth, &mut bracket_depth);
            }
            continue;
        }

        // Plain text line — extend the batch range (includes the \n separator in buf)
        if plain_start.is_none() {
            plain_start = Some(line_offset);
        }
        // The range will extend to the next line's offset (which covers the \n)
    }

    // Flush remaining plain text
    if let Some(start) = plain_start {
        // Write up to end of buffer
        let end = if buf.last() == Some(&b'\n') {
            buf.len()
        } else {
            buf.len()
        };
        if let Err(e) = out.write_all(&buf[start..end]) {
            eprintln!("etoon: write error: {}", e);
            return ExitCode::FAILURE;
        }
    }

    // Flush unclosed JSON block as-is
    if in_json_block {
        let remaining = &buf[block_start..];
        if let Err(e) = out.write_all(remaining).and_then(|_| out.write_all(b"\n")) {
            eprintln!("etoon: write error: {}", e);
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

fn try_encode_json(bytes: &[u8]) -> Option<String> {
    _etoon::toon::encode(bytes).ok()
}

/// Distinguish JSON array from log prefixes like `[request-id] text`.
fn looks_like_json_array(trimmed: &str) -> bool {
    if !trimmed.starts_with('[') {
        return false;
    }
    if let Some(close_pos) = trimmed.find(']') {
        let after_close = trimmed[close_pos + 1..].trim_start();
        if !after_close.is_empty() && !after_close.starts_with(',') && !after_close.starts_with(']')
        {
            return false;
        }
    }
    let after_bracket = trimmed[1..].trim_start();
    if after_bracket.is_empty() {
        return true;
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
    for b in line.bytes() {
        if in_string {
            if b == b'"' && !prev_backslash {
                in_string = false;
            }
            prev_backslash = b == b'\\' && !prev_backslash;
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => *brace_depth += 1,
            b'}' => *brace_depth -= 1,
            b'[' => *bracket_depth += 1,
            b']' => *bracket_depth -= 1,
            _ => {}
        }
    }
}
