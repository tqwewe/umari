// Deterministic event-id derivation matching `Uuid::new_v5(NAMESPACE, key)`
// where `key = correlation_id.bytes() ++ causation_id.bytes() ++ index_be_u32`
// (see `crates/umari/src/command.rs:156-163` and
// `crates/umari/src/lib.rs:116` for the namespace).
//
// Using SHA-1 (pure-JS) for determinism inside the WASM guest. Matches Rust's
// `uuid::Uuid::new_v5` byte-for-byte.

import { uuidToBytes, uuidv5Sync, type Uuid } from "./uuid.js";

export const IDEMPOTENCY_NAMESPACE: Uuid = "e274f2bc-33c5-589f-8643-f3674d86773f";

/**
 * Derive the deterministic id for the `index`-th event emitted under a given
 * (correlation, causation) pair.
 */
export function eventIdFromCausation(
  correlationId: Uuid,
  causationId: Uuid,
  index: number,
): Uuid {
  const cor = uuidToBytes(correlationId);
  const cau = uuidToBytes(causationId);
  const key = new Uint8Array(cor.length + cau.length + 4);
  key.set(cor, 0);
  key.set(cau, cor.length);
  const view = new DataView(key.buffer);
  view.setUint32(cor.length + cau.length, index >>> 0, false);
  return uuidv5Sync(IDEMPOTENCY_NAMESPACE, key);
}
