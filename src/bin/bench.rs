//! Pure Rust benchmark for etoon encoder.
//!
//! Measures the encoder hot path without PyO3/Python overhead.
//! Usage: ./bench [path/to/payload.json]

use std::fs;
use std::time::Instant;

fn bench<F: FnMut() + Copy>(label: &str, mut f: F, rounds: usize, iters: usize) -> f64 {
    for _ in 0..3 {
        f();
    }
    let mut min_us = f64::MAX;
    for _ in 0..rounds {
        let t0 = Instant::now();
        for _ in 0..iters {
            f();
        }
        let dt_us = t0.elapsed().as_nanos() as f64 / iters as f64 / 1000.0;
        if dt_us < min_us {
            min_us = dt_us;
        }
    }
    println!("  {label:40} {min_us:6.2} us");
    min_us
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/neptune_payload.json".into());
    let json_bytes = fs::read(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {}", path, e);
        std::process::exit(1);
    });
    println!("payload: {} ({} bytes)\n", path, json_bytes.len());

    let total = bench(
        "encode (sonic-rs parse + TOON emit)",
        || {
            let _ = _etoon::toon::encode(&json_bytes).unwrap();
        },
        10,
        5000,
    );

    let parse = bench(
        "parse only (sonic-rs)",
        || {
            let _: sonic_rs::Value = sonic_rs::from_slice(&json_bytes).unwrap();
        },
        10,
        5000,
    );

    let emit = total - parse;
    println!();
    println!(
        "  {:40} {:6.2} us  ({:.1}%)",
        "  ↳ parse",
        parse,
        parse / total * 100.0
    );
    println!(
        "  {:40} {:6.2} us  ({:.1}%)",
        "  ↳ emit (estimated)",
        emit,
        emit / total * 100.0
    );
}
