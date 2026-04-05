//! Pure Rust benchmark for etoon encoder.
//!
//! Measures the encoder hot path without PyO3/Python overhead.

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
    let json_bytes = fs::read(&path).expect("read input");
    println!("payload: {} ({} bytes)", path, json_bytes.len());

    // Phase 0: full pipeline (parse + emit)
    let total = bench(
        "encode (parse + emit)",
        || {
            let _ = _etoon::toon::encode(&json_bytes).unwrap();
        },
        10,
        5000,
    );

    // Isolated: parse only
    let parse = bench(
        "parse only (serde_json)",
        || {
            let _: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        },
        10,
        5000,
    );

    println!("  {:40} {:6.2} us  ({:4.1}%)", "  ↳ emit only (estimated)", total - parse, (total - parse) / total * 100.0);
    println!("  {:40} {:6.2} us  ({:4.1}%)", "  ↳ parse only", parse, parse / total * 100.0);

    // Reference: serde_json re-serialize (for comparison)
    let value: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
    bench(
        "serde_json::to_string (reference)",
        || {
            let _ = serde_json::to_string(&value).unwrap();
        },
        10,
        5000,
    );
}
