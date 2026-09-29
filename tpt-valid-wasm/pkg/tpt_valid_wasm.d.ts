/* tslint:disable */
/* eslint-disable */

/**
 * A compiled validator for one JSON Schema (Draft 2020-12 subset).
 *
 * Compile once, validate millions of records.
 *
 * ```js
 * import init, { Validator } from './pkg/tpt_valid_wasm.js';
 * await init();
 * const v = new Validator({ type: 'object', required: ['age'] });
 * const { isValid, errors } = v.validate({ age: 30 });
 * ```
 */
export class Validator {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Compile from a schema JSON string.
     */
    static fromJson(schema: string): Validator;
    /**
     * Boolean-only validation (fail-fast, no error materialization).
     */
    isValid(data: any): boolean;
    /**
     * Compile a JSON Schema from a JS object or a JSON string.
     */
    constructor(schema: any);
    /**
     * Validate an array of values (parallel batch mode). Returns an array
     * of `{ index, isValid, errors }`.
     */
    validateBatch(batch: any): any;
    /**
     * Validate a single value. Returns `{ isValid, errors }` (spec §6.2).
     */
    validate(data: any): any;
    /**
     * Warnings collected during schema compilation (e.g. unknown formats).
     */
    warnings(): Array<any>;
}

/**
 * Library version, e.g. "0.1.0".
 */
export function version(): string;
