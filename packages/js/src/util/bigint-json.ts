// Payload JSON convention: `bigint` fields serialize as decimal `string` on
// the wire. On parse we leave them as strings — user code coerces explicitly
// with `BigInt(value)` since the SDK has no schema to drive auto-coercion.

/**
 * Stringify a payload. Any nested `bigint` is encoded as a string so the
 * Rust side (which deserialises `i64` / `u64` from string-formatted JSON for
 * payload IDs by convention) reads it correctly.
 */
export function stringifyPayload(value: unknown): string {
  return JSON.stringify(value, (_, v) => (typeof v === "bigint" ? v.toString() : v));
}

/**
 * Parse a payload. No automatic bigint coercion — payloads come back as JSON
 * primitives (strings for bigint-by-convention, numbers, booleans, objects,
 * arrays). Callers narrow into typed event data.
 */
export function parsePayload<T = unknown>(raw: string): T {
  return JSON.parse(raw) as T;
}
