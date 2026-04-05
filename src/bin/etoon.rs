//! etoon CLI: read JSON from stdin (or file), write TOON to stdout.
//!
//! Usage:
//!   command | etoon                  # stdin → stdout
//!   etoon input.json                 # file → stdout
//!   etoon input.json -o output.toon  # file → file

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;

    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--output" => {
                output_path = args.next();
                if output_path.is_none() {
                    eprintln!("etoon: -o requires an argument");
                    return ExitCode::FAILURE;
                }
            }
            "-h" | "--help" => {
                println!("etoon: TOON encoder");
                println!("usage: etoon [INPUT] [-o OUTPUT]");
                println!("       command | etoon");
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

    // Read input
    let json_bytes = match input_path {
        Some(path) => match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("etoon: cannot read {}: {}", path, e);
                return ExitCode::FAILURE;
            }
        },
        None => {
            let mut buf = Vec::with_capacity(8192);
            if let Err(e) = io::stdin().lock().read_to_end(&mut buf) {
                eprintln!("etoon: stdin read error: {}", e);
                return ExitCode::FAILURE;
            }
            buf
        }
    };

    // Encode
    let toon = match _etoon::toon::encode(&json_bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("etoon: encode error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Write output
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

    if let Err(e) = out.write_all(toon.as_bytes()).and_then(|_| out.write_all(b"\n")) {
        eprintln!("etoon: write error: {}", e);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
