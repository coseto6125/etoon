// TS SDK timing sidecar for benches/compare.py.
//
// Reads {name, json}[] payloads on stdin, encodes each with @toon-format/toon,
// and writes {name: {us, output}}[] timings to stdout as JSON. Used so the
// Python harness can compare against the official TypeScript encoder.
//
// Run standalone: node benches/ts_bench.mjs < payloads.json

import { encode } from '@toon-format/toon'

function bench(value, iters, rounds) {
  for (let i = 0; i < 3; i++) encode(value) // warm
  let best = Infinity
  for (let r = 0; r < rounds; r++) {
    const t0 = process.hrtime.bigint()
    for (let i = 0; i < iters; i++) encode(value)
    const dt = Number(process.hrtime.bigint() - t0) / iters / 1000 // ns -> us
    if (dt < best) best = dt
  }
  return best
}

const chunks = []
for await (const c of process.stdin) chunks.push(c)
const payloads = JSON.parse(Buffer.concat(chunks).toString('utf8'))

const out = {}
for (const { name, value, iters, rounds } of payloads) {
  out[name] = { us: bench(value, iters ?? 2000, rounds ?? 7), output: encode(value) }
}
process.stdout.write(JSON.stringify(out))
