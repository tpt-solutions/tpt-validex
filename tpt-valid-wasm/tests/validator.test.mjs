// Node.js tests for the tpt-validex WASM bindings.
// Run: npm run test (from tpt-valid-wasm/) — builds the nodejs pkg first.
// The nodejs target self-initializes; browser builds call `await init()`.
import test from 'node:test';
import assert from 'node:assert/strict';

import { Validator, version } from '../pkg/tpt_valid_wasm.js';

const USER_SCHEMA = {
  type: 'object',
  properties: {
    name: { type: 'string', minLength: 1 },
    age: { type: 'integer', minimum: 0, maximum: 150 },
    email: { type: 'string', format: 'email' },
  },
  required: ['name', 'age'],
};

test('validates a single object', () => {
  const v = new Validator(USER_SCHEMA);
  const good = v.validate({ name: 'Alice', age: 30 });
  assert.equal(good.isValid, true);
  assert.deepEqual(good.errors, []);

  const bad = v.validate({ name: '', age: 200 });
  assert.equal(bad.isValid, false);
  const paths = bad.errors.map((e) => e.path).sort();
  assert.deepEqual(paths, ['$.age', '$.name']);
});

test('error shape carries expected/actual/value', () => {
  const v = new Validator(USER_SCHEMA);
  const { errors } = v.validate({ name: 'Bob', age: '25' });
  assert.equal(errors[0].path, '$.age');
  assert.equal(errors[0].expected, 'integer');
  assert.equal(errors[0].actual, 'string');
  assert.equal(errors[0].value, '25');
});

test('format validation', () => {
  const v = new Validator(USER_SCHEMA);
  assert.equal(v.validate({ name: 'A', age: 1, email: 'a@b.com' }).isValid, true);
  assert.equal(v.validate({ name: 'A', age: 1, email: 'nope' }).isValid, false);
});

test('is_valid short-circuits', () => {
  const v = new Validator(USER_SCHEMA);
  assert.equal(v.isValid({ name: 'A', age: 1 }), true);
  assert.equal(v.isValid({}), false);
});

test('batch validation', () => {
  const v = new Validator(USER_SCHEMA);
  const results = v.validateBatch([
    { name: 'Alice', age: 30 },
    { name: 'Bob', age: '25' },
    { name: '', age: 200 },
  ]);
  assert.equal(results.length, 3);
  assert.deepEqual(results.map((r) => r.index), [0, 1, 2]);
  assert.deepEqual(results.map((r) => r.isValid), [true, false, false]);
  assert.equal(results[2].errors.length, 2);
});

test('from_json string constructor', () => {
  const v = Validator.fromJson('{"type": "integer"}');
  assert.equal(v.isValid(42), true);
  assert.equal(v.isValid('42'), false);
});

test('invalid schema surfaces an error', () => {
  assert.throws(() => new Validator({ minimum: 10, maximum: 5 }), /minimum/);
});

test('version string', () => {
  assert.match(version(), /^\d+\.\d+\.\d+$/);
});
