/**
 * Ergonomic TypeScript definitions for the `tpt-validex` npm package.
 *
 * The generated low-level bindings live in `pkg/tpt_valid_wasm.d.ts`; these
 * declarations describe the intended public API (spec §6.2).
 */

/** One structured validation error (spec §5.5). */
export interface ValidationError {
  /** JSON path of the offending value, e.g. "$.address.zip". */
  path: string;
  /** Human-readable description of the violation. */
  message: string;
  /** What the schema expected (type name, bound, format, ...). */
  expected: string;
  /** What was actually found. */
  actual: string;
  /** The offending value, when representable. */
  value?: unknown;
}

/** Result of {@link Validator.validate}. */
export interface ValidateResult {
  isValid: boolean;
  errors: ValidationError[];
}

/** Result of one item in {@link Validator.validateBatch}. */
export interface BatchResult {
  index: number;
  isValid: boolean;
  errors: ValidationError[];
}

/** JSON Schema object (Draft 2020-12 subset) or boolean schema. */
export type JsonSchema = Record<string, unknown> | boolean;

/** Streaming/validation statistics. */
export interface ValidationStats {
  total: number;
  valid: number;
  invalid: number;
  parseErrors: number;
}

/**
 * A compiled validator for one JSON Schema. Compile once, validate
 * millions of records.
 */
export declare class Validator {
  constructor(schema: JsonSchema);
  static fromJson(schema: string): Validator;
  /**
   * Register a custom format assertion for a non-built-in `format` name.
   * The function receives the string under test and its truthiness decides
   * validity. Pass `undefined` to unregister. Unregistered custom formats
   * are ignored, matching JSON Schema annotation semantics.
   */
  registerFormat(name: string, fn: ((value: string) => boolean) | undefined): void;
  validate(data: unknown): ValidateResult;
  isValid(data: unknown): boolean;
  validateBatch(batch: unknown[]): BatchResult[];
  warnings(): string[];
}

/** Library version, e.g. "0.1.0". */
export declare function version(): string;

/**
 * Initialize the WASM module. With a bundler, `await init()` with no
 * arguments; in Node pass the wasm binary or URL explicitly.
 */
export declare function init(input?: RequestInfo | URL | BufferSource): Promise<unknown>;
