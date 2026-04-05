//! Benchmark sonic-rs lazy iterators vs full Value parse.

use sonic_rs::{to_array_iter, to_object_iter, JsonValueTrait};
use std::fs;
use std::time::Instant;

fn bench<F: FnMut() + Copy>(label: &str, mut f: F) -> f64 {
    for _ in 0..3 { f(); }
    let mut min_us = f64::MAX;
    for _ in 0..10 {
        let t0 = Instant::now();
        for _ in 0..5000 { f(); }
        let dt_us = t0.elapsed().as_nanos() as f64 / 5000.0 / 1000.0;
        if dt_us < min_us { min_us = dt_us; }
    }
    println!("  {label:55} {min_us:6.2} us");
    min_us
}

fn main() {
    let json_bytes = fs::read("/tmp/neptune_payload.json").expect("read input");
    println!("payload: {} bytes\n", json_bytes.len());

    // 1. Full Value parse (baseline — what we do now)
    bench("sonic_rs::from_slice -> Value (full eager)", || {
        let _: sonic_rs::Value = sonic_rs::from_slice(&json_bytes).unwrap();
    });

    // 2. Lazy array iter, don't descend
    bench("to_array_iter: iterate top-level only", || {
        let iter = to_array_iter(json_bytes.as_slice());
        let mut n = 0;
        for item in iter {
            let _ = item.unwrap();
            n += 1;
        }
        std::hint::black_box(n);
    });

    // 3. Lazy array iter + descend into each object (typical flow)
    bench("to_array_iter + to_object_iter (lazy walk)", || {
        let iter = to_array_iter(json_bytes.as_slice());
        let mut count = 0;
        for item in iter {
            let item = item.unwrap();
            let item_bytes = item.as_raw_str().as_bytes();
            let inner = to_object_iter(item_bytes);
            for kv in inner {
                let (_k, _v) = kv.unwrap();
                count += 1;
            }
        }
        std::hint::black_box(count);
    });

    // 4. Lazy walk + access each scalar's as_raw (to simulate emit)
    bench("lazy walk + access raw bytes of each value", || {
        let iter = to_array_iter(json_bytes.as_slice());
        let mut total_len = 0usize;
        for item in iter {
            let item = item.unwrap();
            let item_bytes = item.as_raw_str().as_bytes();
            let inner = to_object_iter(item_bytes);
            for kv in inner {
                let (_k, v) = kv.unwrap();
                total_len += v.as_raw_str().len();
            }
        }
        std::hint::black_box(total_len);
    });
}
