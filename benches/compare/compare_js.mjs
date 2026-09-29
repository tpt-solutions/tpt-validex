// Comparison benchmark: tpt-validex (WASM) vs zod (Node.js).
// Same dataset and schema shape as the Rust criterion suite.
//
// Setup:
//   cd tpt-valid-wasm && npm run build:node
//   npm install --no-save zod@3
// Run:
//   node ../benches/compare/compare_js.mjs

import { performance } from 'node:perf_hooks';
import { createRequire } from 'node:module';

// Resolve relative to this file so the benchmark can run from any cwd.
const wasmPkgUrl = new URL('../../tpt-valid-wasm/pkg/tpt_valid_wasm.js', import.meta.url);
const requireFromWasm = createRequire(new URL('../../tpt-valid-wasm/package.json', import.meta.url));

const BATCH = 10_000;

const SCHEMA = {
  type: 'object',
  properties: {
    name: { type: 'string', minLength: 1, maxLength: 100 },
    age: { type: 'integer', minimum: 0, maximum: 150 },
    email: { type: 'string', format: 'email' },
    tags: { type: 'array', items: { type: 'string' }, maxItems: 10 },
  },
  required: ['name', 'age'],
};

const makeBatch = () =>
  Array.from({ length: BATCH }, (_, i) => ({
    name: `user${i}`,
    age: i % 150,
    email: `user${i}@example.com`,
    tags: ['a', 'b'],
  }));

const bench = (fn, data, minTime = 1000) => {
  const start = performance.now();
  let n = 0;
  while (true) {
    fn(data);
    n += data.length;
    if (performance.now() - start >= minTime) return (n * 1000) / (performance.now() - start);
  }
};

const results = {};

// tpt-validex (WASM) ------------------------------------------------------
const { Validator } = await import(wasmPkgUrl);
const validator = new Validator(SCHEMA);
results['tpt-validex single'] = benchSingle((v) => validator.isValid(v), makeBatch()[0]);
results['tpt-validex batch'] = bench((b) => validator.validateBatch(b), makeBatch());

// zod ---------------------------------------------------------------------
try {
  const { z } = requireFromWasm('zod');
  const user = z.object({
    name: z.string().min(1).max(100),
    age: z.number().int().min(0).max(150),
    email: z.string().email().optional(),
    tags: z.array(z.string()).max(10).optional(),
  });

  results['zod single'] = benchSingle((v) => user.safeParse(v), makeBatch()[0]);
  results['zod batch'] = bench((b) => {
    for (const x of b) user.safeParse(x);
  }, makeBatch());
} catch {
  console.log('zod not installed; skipping (npm install --no-save zod@3)');
}

function benchSingle(fn, value, minTime = 1000) {
  const start = performance.now();
  let n = 0;
  while (true) {
    fn(value);
    n += 1;
    if (performance.now() - start >= minTime) return (n * 1000) / (performance.now() - start);
  }
}

console.log(`\nDataset: flat user object, batch of ${BATCH.toLocaleString()} (wasm module)`);
console.log(`${'Benchmark'.padEnd(28)}${'objects/sec'.padStart(15)}`);
console.log('-'.repeat(43));
for (const [name, ops] of Object.entries(results)) {
  console.log(`${name.padEnd(28)}${ops.toLocaleString('en-US', { maximumFractionDigits: 0 }).padStart(15)}`);
}
