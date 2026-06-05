// High-level SQLite namespace wrapping `umari:sqlite/connection` and
// `umari:sqlite/statement`. Mirrors `crates/umari/src/sqlite.rs`.

import * as conn from "umari:sqlite/connection@0.1.0";
import { Stmt } from "umari:sqlite/statement@0.1.0";
import type {
  Value as WitValue,
  Row as WitRow,
  SqliteError as WitSqliteError,
} from "umari:sqlite/types@0.1.0";

/** SQL-side error wrapping constraint violations. Other errors trap. */
export type SqliteError = WitSqliteError;

/**
 * Acceptable parameter types for a prepared statement or `execute`.
 * - `null` → NULL
 * - `bigint` → INTEGER (s64)
 * - `number` → INTEGER if integer-valued, REAL otherwise
 * - `boolean` → INTEGER 0/1
 * - `string` → TEXT
 * - `Uint8Array` → BLOB
 */
export type SqliteParam =
  | null
  | bigint
  | number
  | boolean
  | string
  | Uint8Array;

function lower(v: SqliteParam): WitValue {
  if (v === null) return { tag: "null" };
  if (typeof v === "bigint") return { tag: "integer", val: v };
  if (typeof v === "number") {
    if (Number.isInteger(v) && Math.abs(v) <= Number.MAX_SAFE_INTEGER) {
      return { tag: "integer", val: BigInt(v) };
    }
    return { tag: "real", val: v };
  }
  if (typeof v === "boolean") return { tag: "integer", val: v ? 1n : 0n };
  if (typeof v === "string") return { tag: "text", val: v };
  if (v instanceof Uint8Array) return { tag: "blob", val: v };
  throw new Error(`unsupported sqlite param: ${typeof v}`);
}

function lowerParams(params: readonly SqliteParam[] | undefined): WitValue[] {
  if (!params) return [];
  return params.map(lower);
}

/** Coerce a `Value` variant to a JS-friendly primitive. */
function liftValue(v: WitValue): bigint | number | string | Uint8Array | null {
  switch (v.tag) {
    case "null":
      return null;
    case "integer":
      return v.val;
    case "real":
      return v.val;
    case "text":
      return v.val;
    case "blob":
      return v.val;
  }
}

/** A row returned by a query. Access columns via `.get(name | index)`. */
export class Row {
  readonly columns: ReadonlyArray<{ name: string; value: WitValue }>;

  constructor(raw: WitRow) {
    this.columns = raw.columns;
  }

  /**
   * Read a column. If `T` is given, coerces:
   * - `'bigint'` — `bigint` (integer or text coerced via `BigInt`)
   * - `'number'` — `number` (range-checked; throws if outside safe integer)
   * - `'string'` — `string` (numbers stringify)
   * - `'boolean'` — `boolean` (0/1 → false/true)
   * - `'uint8array'` — `Uint8Array`
   * - `'date'` — `Date` (timestamp millis or RFC3339 text)
   * - `undefined` — raw `bigint | number | string | Uint8Array | null`
   */
  get(column: string | number): bigint | number | string | Uint8Array | null;
  get(column: string | number, as: "bigint"): bigint | null;
  get(column: string | number, as: "number"): number | null;
  get(column: string | number, as: "string"): string | null;
  get(column: string | number, as: "boolean"): boolean | null;
  get(column: string | number, as: "uint8array"): Uint8Array | null;
  get(column: string | number, as: "date"): Date | null;
  get(column: string | number, as?: string): unknown {
    const raw = this.#raw(column);
    const v = liftValue(raw);
    if (v === null) return null;
    switch (as) {
      case undefined:
        return v;
      case "bigint":
        if (typeof v === "bigint") return v;
        if (typeof v === "number") return BigInt(v);
        if (typeof v === "string") return BigInt(v);
        throw new Error(`cannot coerce ${typeof v} to bigint`);
      case "number":
        if (typeof v === "number") return v;
        if (typeof v === "bigint") {
          if (v > BigInt(Number.MAX_SAFE_INTEGER) || v < BigInt(Number.MIN_SAFE_INTEGER)) {
            throw new Error(`bigint ${v} out of safe number range`);
          }
          return Number(v);
        }
        if (typeof v === "string") return Number(v);
        throw new Error(`cannot coerce ${typeof v} to number`);
      case "string":
        if (typeof v === "string") return v;
        if (typeof v === "bigint" || typeof v === "number") return v.toString();
        throw new Error(`cannot coerce ${typeof v} to string`);
      case "boolean":
        if (typeof v === "bigint") return v !== 0n;
        if (typeof v === "number") return v !== 0;
        throw new Error(`cannot coerce ${typeof v} to boolean`);
      case "uint8array":
        if (v instanceof Uint8Array) return v;
        throw new Error(`cannot coerce ${typeof v} to Uint8Array`);
      case "date":
        if (typeof v === "bigint") return new Date(Number(v));
        if (typeof v === "number") return new Date(v);
        if (typeof v === "string") return new Date(v);
        throw new Error(`cannot coerce ${typeof v} to Date`);
      default:
        throw new Error(`unknown coercion: ${as}`);
    }
  }

  /** Number of columns. */
  get length(): number {
    return this.columns.length;
  }

  /** Iterate columns by name. */
  toObject(): Record<string, bigint | number | string | Uint8Array | null> {
    const out: Record<string, bigint | number | string | Uint8Array | null> = {};
    for (const col of this.columns) out[col.name] = liftValue(col.value);
    return out;
  }

  #raw(column: string | number): WitValue {
    if (typeof column === "number") {
      const c = this.columns[column];
      if (!c) throw new Error(`no column at index ${column}`);
      return c.value;
    }
    for (const c of this.columns) {
      if (c.name === column) return c.value;
    }
    throw new Error(`no column named ${column}`);
  }
}

/** Prepared statement wrapping `umari:sqlite/statement.stmt`. */
export class PreparedStatement {
  readonly #stmt: Stmt;
  constructor(sql: string) {
    this.#stmt = new Stmt(sql);
  }
  /** Returns rows affected. */
  execute(params?: readonly SqliteParam[]): bigint {
    return this.#stmt.execute(lowerParams(params));
  }
  query(params?: readonly SqliteParam[]): Row[] {
    return this.#stmt.query(lowerParams(params)).map((r) => new Row(r));
  }
  queryOne(params?: readonly SqliteParam[]): Row {
    return new Row(this.#stmt.queryOne(lowerParams(params)));
  }
  queryRow(params?: readonly SqliteParam[]): Row | undefined {
    const r = this.#stmt.queryRow(lowerParams(params));
    return r ? new Row(r) : undefined;
  }
}

// Top-level connection-scoped helpers.

/** Run a statement. Returns rows affected. Throws `SqliteError` on constraint violation. */
export function execute(sql: string, params?: readonly SqliteParam[]): bigint {
  return conn.execute(sql, lowerParams(params));
}

/** Run a batch of statements separated by `;`. */
export function executeBatch(sql: string): void {
  conn.executeBatch(sql);
}

/** Return the last `INSERT` rowid, if any. */
export function lastInsertRowid(): bigint | undefined {
  return conn.lastInsertRowid();
}

/** Run a query expected to return exactly one row. Traps if 0 or >1. */
export function queryOne(sql: string, params?: readonly SqliteParam[]): Row {
  return new Row(conn.queryOne(sql, lowerParams(params)));
}

/** Run a query returning at most one row. */
export function queryRow(sql: string, params?: readonly SqliteParam[]): Row | undefined {
  const r = conn.queryRow(sql, lowerParams(params));
  return r ? new Row(r) : undefined;
}

/** Run a query and return all rows. Implemented via a prepared statement. */
export function query(sql: string, params?: readonly SqliteParam[]): Row[] {
  return new PreparedStatement(sql).query(params);
}

/** Prepare a statement for repeated execution. */
export function prepare(sql: string): PreparedStatement {
  return new PreparedStatement(sql);
}

// Namespace-style export for `import { sqlite } from '@umari/js/sqlite'`.
export const sqlite = {
  execute,
  executeBatch,
  lastInsertRowid,
  queryOne,
  queryRow,
  query,
  prepare,
} as const;
