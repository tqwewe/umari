// db.ts — SQLite wrapper over WIT sqlite imports
// Import paths are resolved by ComponentizeJS at build time from the WIT world.
// TypeScript resolves these via tsconfig paths → src/generated/sqlite/interfaces/
import * as connection from "umari:sqlite/connection@0.1.0";
import { Stmt } from "umari:sqlite/statement@0.1.0";
import type { Column, Row, Value } from "umari:sqlite/types@0.1.0";
import type { SqliteDb, SqliteRow, SqliteStatement } from "./types.ts";

function toWitValue(v: unknown): Value {
  if (v === null || v === undefined) return { tag: "null" };
  if (typeof v === "bigint") return { tag: "integer", val: v };
  if (typeof v === "number") {
    if (Number.isInteger(v)) return { tag: "integer", val: BigInt(v) };
    return { tag: "real", val: v };
  }
  if (typeof v === "string") return { tag: "text", val: v };
  if (v instanceof Uint8Array) return { tag: "blob", val: v };
  throw new Error(`unsupported sqlite param type: ${typeof v}`);
}

function fromWitColumn(col: Column): [string, SqliteRow[string]] {
  const v = col.value;
  switch (v.tag) {
    case "null":    return [col.name, null];
    case "integer": return [col.name, v.val];
    case "real":    return [col.name, v.val];
    case "text":    return [col.name, v.val];
    case "blob":    return [col.name, v.val];
  }
}

function fromWitRow(row: Row): SqliteRow {
  const obj: SqliteRow = {};
  for (const col of row.columns) {
    const [name, value] = fromWitColumn(col);
    obj[name] = value;
  }
  return obj;
}

export function createDb(): SqliteDb {
  return {
    execute(sql, params = []) {
      return Number(connection.execute(sql, params.map(toWitValue)));
    },

    executeBatch(sql) {
      connection.executeBatch(sql);
    },

    queryOne(sql, params = []) {
      const row = connection.queryOne(sql, params.map(toWitValue));
      return row !== undefined ? fromWitRow(row) : null;
    },

    queryRow(sql, params = []) {
      const row = connection.queryRow(sql, params.map(toWitValue));
      return row !== undefined ? fromWitRow(row) : null;
    },

    query(sql, params = []) {
      const stmt = new Stmt(sql);
      return stmt.query(params.map(toWitValue)).map(fromWitRow);
    },

    lastInsertRowId() {
      return connection.lastInsertRowid();
    },

    prepare(sql): SqliteStatement {
      const stmt = new Stmt(sql);
      return {
        execute(params = []) {
          return Number(stmt.execute(params.map(toWitValue)));
        },
        query(params = []) {
          return stmt.query(params.map(toWitValue)).map(fromWitRow);
        },
        queryOne(params = []) {
          const row = stmt.queryOne(params.map(toWitValue));
          return row !== undefined ? fromWitRow(row) : null;
        },
        queryRow(params = []) {
          const row = stmt.queryRow(params.map(toWitValue));
          return row !== undefined ? fromWitRow(row) : null;
        },
      };
    },
  };
}
