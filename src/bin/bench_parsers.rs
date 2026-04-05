//! Benchmark alternative JSON parsers, including compat mode.

use std::fs;
use std::time::Instant;

fn bench<F: FnMut() + Copy>(label: &str, mut f: F) -> f64 {
    for _ in 0..3 {
        f();
    }
    let mut min_us = f64::MAX;
    for _ in 0..10 {
        let t0 = Instant::now();
        for _ in 0..5000 {
            f();
        }
        let dt_us = t0.elapsed().as_nanos() as f64 / 5000.0 / 1000.0;
        if dt_us < min_us {
            min_us = dt_us;
        }
    }
    println!("  {label:50} {min_us:6.2} us");
    min_us
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/neptune_payload.json".into());
    let json_bytes = fs::read(&path).expect("read input");
    println!("payload: {} bytes\n", json_bytes.len());

    bench("serde_json::from_slice -> serde_json::Value", || {
        let _: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
    });

    bench("sonic_rs::from_slice -> sonic_rs::Value", || {
        let _: sonic_rs::Value = sonic_rs::from_slice(&json_bytes).unwrap();
    });

    bench("sonic_rs::from_slice -> serde_json::Value", || {
        let _: serde_json::Value = sonic_rs::from_slice(&json_bytes).unwrap();
    });
}

// see main() in bench_parsers
